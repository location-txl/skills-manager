//! Controlled executor for the Vercel `skills` CLI.
//!
//! Every CLI invocation from the GUI goes through here. The rules are fixed
//! by the refactor contract:
//! - argv-array invocation only, never a shell, never string concatenation;
//! - a whitelisted environment (tokens pass through but are never logged);
//! - `stdin` is null so an unexpected interactive prompt dies instead of
//!   hanging the UI;
//! - stdout/stderr are capped, every call has a timeout and can be cancelled;
//! - the whole process group is killed so git/ssh children die with node.
//!
//! Also mirrors the CLI's `sanitizeName` (installer.ts) — lock keys keep the
//! original names while folders are sanitized, so matching the two requires
//! the exact same transformation.

use crate::core::error::AppError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::watch;

/// Default cap for each output stream (CLI --json listings stay far below;
/// human-mode update logs are truncated instead of ballooning memory).
pub const DEFAULT_OUTPUT_CAP: usize = 2 * 1024 * 1024;

/// Mirrors installer.ts `sanitizeName()`: lowercase, collapse everything
/// outside `[a-z0-9._]` into single hyphens, strip leading/trailing dots and
/// hyphens, cap at 255 chars, fall back to `unnamed-skill`.
pub fn cli_sanitize_name(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut pending_hyphen = false;
    for ch in lower.chars() {
        let allowed =
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '_';
        if allowed {
            if pending_hyphen {
                out.push('-');
                pending_hyphen = false;
            }
            out.push(ch);
        } else {
            pending_hyphen = true;
        }
    }
    let trimmed = out.trim_matches(|c| c == '.' || c == '-');
    // After the replacement pass the string is pure ASCII, so a byte cut at
    // 255 matches JS `substring(0, 255)`.
    let mut sanitized: String = trimmed.as_bytes()[..trimmed.len().min(255)]
        .iter()
        .map(|b| *b as char)
        .collect();
    if sanitized.is_empty() {
        sanitized = "unnamed-skill".to_string();
    }
    sanitized
}

/// How the CLI was located, surfaced in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliSource {
    /// User-configured absolute path (Settings page).
    UserOverride,
    /// Found on the app process's PATH.
    Path,
    /// Found in a well-known install location (npm global dirs, homebrew…).
    CandidateDir,
}

/// A located, version-checked CLI runtime: we invoke
/// `<node_path> <cli_entry> <args…>` directly, which avoids every shell and
/// Windows `.cmd` shim problem.
#[derive(Debug, Clone, PartialEq)]
pub struct CliRuntime {
    pub node_path: PathBuf,
    /// Absolute path to the CLI's JS entry (`bin/cli.mjs` inside the package).
    pub cli_entry: PathBuf,
    /// Version reported by `skills --version`.
    pub version: String,
    pub source: CliSource,
}

/// One CLI invocation.
#[derive(Debug, Clone)]
pub struct CliCommandSpec {
    pub args: Vec<String>,
    /// Working directory. `None` means a neutral directory (temp dir) — never
    /// the GUI's own project root, so project-scoped commands can't misfire.
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
    pub output_cap: usize,
    /// Environment overrides (isolation); empty for the real app.
    pub env: ChildEnv,
}

impl CliCommandSpec {
    pub fn new(args: Vec<String>) -> Self {
        Self {
            args,
            cwd: None,
            timeout: Duration::from_secs(60),
            output_cap: DEFAULT_OUTPUT_CAP,
            env: ChildEnv::default(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_env(mut self, env: ChildEnv) -> Self {
        self.env = env;
        self
    }
}

/// Result of a CLI run. `exit_code` is `None` when the process was killed by
/// a signal. `tim_out`/`cancelled` say why.
#[derive(Debug, Clone)]
pub struct CliOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub duration: Duration,
}

impl CliOutcome {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Cooperative cancellation for long CLI runs. The command layer keeps the
/// sender, this handle is polled by the executor.
#[derive(Clone)]
pub struct CancelHandle {
    rx: Arc<watch::Receiver<bool>>,
}

impl CancelHandle {
    pub fn new() -> (watch::Sender<bool>, Self) {
        let (tx, rx) = watch::channel(false);
        (tx, Self { rx: Arc::new(rx) })
    }

    async fn wait(&self) {
        let mut rx = (*self.rx).clone();
        loop {
            if *rx.borrow() {
                return;
            }
            if rx.changed().await.is_err() {
                // All senders dropped without cancelling: never resolves.
                std::future::pending::<()>().await;
            }
        }
    }
}

/// Environment context for a CLI child process. `None` fields inherit from
/// this app process; `Some` values override (used by tests to run the CLI
/// against an isolated HOME without touching the process environment).
#[derive(Debug, Clone, Default)]
pub struct ChildEnv {
    /// Override `HOME`/`USERPROFILE` for the child.
    pub home: Option<PathBuf>,
    /// Override `XDG_STATE_HOME`: `Some(None)` clears it, `Some(Some(v))`
    /// sets it, `None` inherits.
    pub xdg_state_home: Option<Option<String>>,
}

/// Build the whitelisted environment for a CLI child process. `node_dir` is
/// prepended to PATH so the shebang-style tools the CLI spawns (git, ssh)
/// and node itself resolve the same way they would in a terminal.
pub fn build_cli_env(node_dir: Option<&Path>, overrides: &ChildEnv) -> HashMap<String, String> {
    const PASS_THROUGH: &[&str] = &[
        "HOME",
        "LANG",
        "TZ",
        "TMPDIR",
        "TEMP",
        "TMP",
        "XDG_STATE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        // Agent homes the CLI honors when computing agent directories.
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "VIBE_HOME",
        "HERMES_HOME",
        "AUTOHAND_HOME",
        "GROK_HOME",
        "SARVAM_HOME",
        "FLATPAK_XDG_CONFIG_HOME",
        // Windows essentials: node and npm need these.
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "APPDATA",
        "LOCALAPPDATA",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "COMSPEC",
        // Networking.
        "http_proxy",
        "https_proxy",
        "no_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "NO_PROXY",
        // GitHub auth for private sources; passed through, never logged.
        "GITHUB_TOKEN",
        "GH_TOKEN",
    ];

    let mut env: HashMap<String, String> = HashMap::new();
    for key in PASS_THROUGH {
        if let Ok(value) = std::env::var(key) {
            env.insert(key.to_string(), value);
        }
    }
    // Windows env lookup is case-insensitive; pick up variants like "Path".
    if cfg!(windows) {
        for (key, value) in std::env::vars() {
            let upper = key.to_ascii_uppercase();
            if PASS_THROUGH.contains(&upper.as_str()) && !env.contains_key(&upper) {
                env.insert(upper, value);
            }
        }
    }
    // Test/isolation overrides come after inheritance so they win.
    if let Some(home) = &overrides.home {
        env.insert("HOME".to_string(), home.display().to_string());
        if cfg!(windows) {
            env.insert("USERPROFILE".to_string(), home.display().to_string());
        }
        // An injected home must not keep the real XDG override alive.
        env.remove("XDG_STATE_HOME");
    }
    match &overrides.xdg_state_home {
        Some(Some(value)) => {
            env.insert("XDG_STATE_HOME".to_string(), value.clone());
        }
        Some(None) => {
            env.remove("XDG_STATE_HOME");
        }
        None => {}
    }
    if let Some(dir) = node_dir {
        let existing = std::env::var("PATH").unwrap_or_default();
        let dir_str = dir.to_string_lossy().to_string();
        if !existing.split(std::path::MAIN_SEPARATOR_STR).any(|p| p == dir_str) {
            let joined = if existing.is_empty() {
                dir_str
            } else if cfg!(windows) {
                format!("{};{}", dir_str, existing)
            } else {
                format!("{}:{}", dir_str, existing)
            };
            env.insert("PATH".to_string(), joined);
        } else {
            env.insert("PATH".to_string(), existing);
        }
    } else if let Ok(path) = std::env::var("PATH") {
        env.insert("PATH".to_string(), path);
    }
    // No ANSI in captured output; no telemetry network calls (also avoids
    // the CLI's up-to-5s telemetry flush on exit).
    env.insert("NO_COLOR".to_string(), "1".into());
    env.insert("DISABLE_TELEMETRY".to_string(), "1".into());
    env
}

/// Kill an entire process group (unix) or process tree (Windows).
fn kill_process_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    {
        // Negative pid targets the process group set up by `process_group(0)`.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        // taskkill with a fixed argv; no shell involved.
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .status();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
    }
}

/// Neutral working directory for global-scope commands: a temp dir, so a
/// project-scoped default can never fire against the GUI's own checkout.
pub fn neutral_cwd() -> PathBuf {
    std::env::temp_dir()
}

/// Run one CLI command to completion (or timeout/cancel). Never retries and
/// never interprets the output — parsing belongs to callers.
pub async fn run_cli_command(
    runtime: &CliRuntime,
    spec: &CliCommandSpec,
    cancel: Option<&CancelHandle>,
) -> Result<CliOutcome, AppError> {
    let started = Instant::now();
    let mut cmd = Command::new(&runtime.node_path);
    cmd.arg(&runtime.cli_entry)
        .args(&spec.args)
        .env_clear()
        .envs(build_cli_env(runtime.node_path.parent(), &spec.env))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: the GUI never flashes a console.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let cwd = spec.cwd.clone().unwrap_or_else(neutral_cwd);
    cmd.current_dir(&cwd);

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::internal(format!("failed to start skills CLI: {e}")))?;
    let pid = child.id();

    let mut stdout_pipe = child.stdout.take().expect("stdout piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr piped");
    let cap = spec.output_cap;
    let stdout_task =
        tokio::spawn(async move { read_capped(&mut stdout_pipe, cap).await });
    let stderr_task =
        tokio::spawn(async move { read_capped(&mut stderr_pipe, cap).await });

    let deadline = tokio::time::Instant::from_std(started + spec.timeout);
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        tokio::select! {
            status = child.wait() => break status.map_err(AppError::io)?,
            _ = tokio::time::sleep_until(deadline), if !timed_out => {
                timed_out = true;
                kill_process_tree(pid);
                // Fall through: wait for the (killed) child below.
            }
            _ = async { match cancel { Some(h) => h.wait().await, None => std::future::pending::<()>().await } }, if !cancelled => {
                cancelled = true;
                kill_process_tree(pid);
            }
        }
        if timed_out || cancelled {
            break child.wait().await.map_err(AppError::io)?;
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    Ok(CliOutcome {
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out,
        cancelled,
        duration: started.elapsed(),
    })
}

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let room = cap - buf.len();
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                }
                // Past the cap we keep draining but stop storing.
            }
        }
    }
    buf
}

/// Parse the CLI's `--json` stdout. The CLI promises exactly one JSON value
/// on stdout in JSON mode; anything else is an error, not a guess.
pub fn parse_json_stdout<T: serde::de::DeserializeOwned>(
    outcome: &CliOutcome,
) -> Result<T, AppError> {
    if outcome.timed_out {
        return Err(AppError::internal("skills CLI timed out"));
    }
    if outcome.cancelled {
        return Err(AppError::cancelled("skills CLI run cancelled"));
    }
    let trimmed = outcome.stdout.trim();
    if trimmed.is_empty() {
        return Err(AppError::internal(format!(
            "skills CLI returned no JSON output (exit code {:?}): {}",
            outcome.exit_code,
            truncate_head(outcome.stderr.trim(), 200)
        )));
    }
    serde_json::from_str(trimmed).map_err(|e| {
        AppError::internal(format!(
            "skills CLI returned unparseable JSON: {e}; stdout head: {}",
            truncate_head(trimmed, 200)
        ))
    })
}

fn truncate_head(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_matches_cli_rules() {
        assert_eq!(cli_sanitize_name("web-design"), "web-design");
        assert_eq!(cli_sanitize_name("Web Design"), "web-design");
        assert_eq!(cli_sanitize_name("ce:review"), "ce-review");
        assert_eq!(cli_sanitize_name("../escape/attempt"), "escape-attempt");
        assert_eq!(cli_sanitize_name(".hidden"), "hidden");
        assert_eq!(cli_sanitize_name("-leading-"), "leading");
        assert_eq!(cli_sanitize_name("a  --  b"), "a-b");
        assert_eq!(cli_sanitize_name("trailing..."), "trailing");
        assert_eq!(cli_sanitize_name("中文技能"), "unnamed-skill");
        assert_eq!(cli_sanitize_name(""), "unnamed-skill");
        assert_eq!(cli_sanitize_name("Git Review Before Commit"), "git-review-before-commit");
    }

    #[test]
    fn sanitize_caps_at_255_chars() {
        let long = "a".repeat(300);
        assert_eq!(cli_sanitize_name(&long).len(), 255);
    }

    #[test]
    fn sanitize_collapses_unicode_runs_like_the_cli() {
        // JS replaces any disallowed run with a single hyphen.
        assert_eq!(cli_sanitize_name("foo:::bar"), "foo-bar");
        assert_eq!(cli_sanitize_name("foo  bar"), "foo-bar");
        assert_eq!(cli_sanitize_name("a/\\b"), "a-b");
    }

    #[test]
    fn env_whitelist_drops_unknown_variables() {
        // SAFETY: tests run in parallel; use a unique name to avoid clashes.
        let key = "SKM_TEST_SECRET_LEAK_CHECK";
        std::env::set_var(key, "boom");
        let env = build_cli_env(None, &ChildEnv::default());
        std::env::remove_var(key);
        assert!(!env.contains_key(key));
        assert_eq!(env.get("NO_COLOR").map(String::as_str), Some("1"));
        assert_eq!(
            env.get("DISABLE_TELEMETRY").map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn env_whitelist_prepends_node_dir_to_path() {
        let env = build_cli_env(Some(Path::new("/opt/node/bin")), &ChildEnv::default());
        let path = env.get("PATH").expect("path set");
        assert!(path.starts_with("/opt/node/bin"), "got: {path}");
    }

    #[test]
    fn env_overrides_inject_home_and_clear_xdg() {
        // SAFETY: unique values, restored immediately.
        std::env::set_var("XDG_STATE_HOME", "/real/xdg");
        let home = tempfile::tempdir().unwrap();
        let env = build_cli_env(
            None,
            &ChildEnv {
                home: Some(home.path().to_path_buf()),
                xdg_state_home: Some(None),
            },
        );
        std::env::remove_var("XDG_STATE_HOME");
        assert_eq!(
            env.get("HOME").map(String::as_str),
            Some(home.path().to_str().unwrap())
        );
        assert!(!env.contains_key("XDG_STATE_HOME"));
    }

    #[tokio::test]
    async fn child_sees_injected_home() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        let home = tempfile::tempdir().unwrap();
        let spec = CliCommandSpec::new(vec![
            "console.log(process.env.HOME)".into(),
        ])
        .with_env(ChildEnv {
            home: Some(home.path().to_path_buf()),
            xdg_state_home: Some(None),
        });
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert_eq!(out.stdout.trim(), home.path().to_str().unwrap());
    }

    fn node_runtime_for_tests() -> Option<CliRuntime> {
        let node = which_node().map(|p| p.to_path_buf())?;
        // Use `node -e` as a stand-in CLI entry: `-e <script>` then extra args.
        Some(CliRuntime {
            node_path: node,
            cli_entry: PathBuf::from("-e"),
            version: "test".into(),
            source: CliSource::Path,
        })
    }

    fn which_node() -> Option<&'static Path> {
        static NODE: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        NODE.get_or_init(|| {
            let node = if cfg!(windows) { "node.exe" } else { "node" };
            let path = std::env::var_os("PATH")?;
            for dir in std::env::split_paths(&path) {
                let candidate = dir.join(node);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            None
        })
        .as_deref()
    }

    #[tokio::test]
    async fn captures_stdout_and_exit_code() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        let spec = CliCommandSpec::new(vec![
            "console.log(JSON.stringify({ok: true}))".into(),
        ]);
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.trim(), r#"{"ok":true}"#);
        let parsed: serde_json::Value = parse_json_stdout(&out).unwrap();
        assert_eq!(parsed["ok"], true);
    }

    #[tokio::test]
    async fn timeout_kills_the_process() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        let spec = CliCommandSpec::new(vec!["setTimeout(()=>{}, 60000)".into()])
            .with_timeout(Duration::from_millis(300));
        let started = Instant::now();
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert!(out.timed_out);
        assert_ne!(out.exit_code, Some(0));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn cancel_flag_kills_the_process() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        let (tx, handle) = CancelHandle::new();
        let spec = CliCommandSpec::new(vec!["setTimeout(()=>{}, 60000)".into()])
            .with_timeout(Duration::from_secs(30));
        let run = tokio::spawn(async move {
            run_cli_command(&rt, &spec, Some(&handle)).await.unwrap()
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = tx.send(true);
        let out = run.await.unwrap();
        assert!(out.cancelled);
        assert!(!out.timed_out);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn process_group_children_die_too() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        // Parent spawns a child (which reports its pid on our stdout), then
        // both hang. A group kill must take the child down with the parent.
        let script = r#"
            const { spawn } = require('child_process');
            spawn(process.execPath, ['-e', 'console.log("CHILD_PID:" + process.pid); setInterval(()=>{}, 1000);'],
                  { stdio: ['ignore', 'inherit', 'inherit'] });
            setInterval(()=>{}, 1000);
        "#;
        let spec = CliCommandSpec::new(vec![script.into()])
            .with_timeout(Duration::from_millis(400));
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert!(out.timed_out);
        let pid_line = out
            .stdout
            .lines()
            .find(|l| l.starts_with("CHILD_PID:"))
            .expect("child reported its pid");
        let child_pid: i32 = pid_line
            .trim_start_matches("CHILD_PID:")
            .parse()
            .unwrap();
        // Signal 0 probes existence; a dead child returns -1/ESRCH.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let alive = unsafe { libc::kill(child_pid, 0) } == 0;
        assert!(!alive, "child {child_pid} survived the group kill");
    }

    #[tokio::test]
    async fn output_is_capped() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        let script = r#"
            const chunk = 'x'.repeat(1024);
            for (let i = 0; i < 10; i++) process.stdout.write(chunk);
        "#;
        let spec = CliCommandSpec {
            args: vec![script.into()],
            cwd: None,
            timeout: Duration::from_secs(10),
            output_cap: 2048,
            env: ChildEnv::default(),
        };
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.stdout.len(), 2048);
    }

    #[tokio::test]
    async fn stdin_is_null_so_prompts_die_fast() {
        let Some(rt) = node_runtime_for_tests() else {
            eprintln!("node not found; skipping");
            return;
        };
        // A prompt reads stdin; with stdin null it sees EOF immediately and
        // node exits instead of hanging.
        let script = r#"
            const data = require('fs').readFileSync(0, 'utf8');
            console.log('read:', JSON.stringify(data));
        "#;
        let spec = CliCommandSpec::new(vec![script.into()]).with_timeout(Duration::from_secs(10));
        let out = run_cli_command(&rt, &spec, None).await.unwrap();
        assert_eq!(out.exit_code, Some(0));
        assert!(out.stdout.contains("read: \"\""));
    }
}
