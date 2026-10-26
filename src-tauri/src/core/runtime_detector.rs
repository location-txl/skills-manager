//! Runtime detection: locate Node.js and the Vercel `skills` CLI, verify
//! they run, and decide what the GUI may do.
//!
//! Design constraints (refactor contract):
//! - The app requires a user-installed Node.js + `skills` CLI; it never
//!   downloads or upgrades them itself.
//! - macOS apps launched from Finder see a minimal PATH, so detection also
//!   probes well-known install locations (nvm, volta, homebrew, npm global).
//! - We invoke the CLI as `node <…>/bin/cli.mjs <args>` with absolute paths,
//!   never a shell and never a `.cmd` shim.
//! - Version policy: the write contract is anchored at `skills@1.7.4`
//!   (`add --json` requires 1.7.0+). Older CLIs still serve reads
//!   (`ls --json` exists since 1.5.x), but writes are disabled with an
//!   upgrade hint rather than run through untested paths.

use super::skills_cli::{build_cli_env, CliRuntime, CliSource};
use semver::Version;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Minimum CLI version whose write operations this app will drive.
/// 1.7.4 is the version the contract tests are anchored to; 1.7.0 is the
/// floor because `add --json` appears there.
pub const MIN_WRITE_VERSION: Version = Version::new(1, 7, 0);

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInfo {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CliInfo {
    pub runtime: CliRuntime,
    /// Parsed CLI version; `None` when `--version` output was not a version.
    pub version: Option<Version>,
    /// True when write operations may run against this CLI.
    pub writes_supported: bool,
}

/// Everything the Settings/Diagnostics page shows in one shot.
#[derive(Debug, Clone, Default)]
pub struct RuntimeStatus {
    pub node: Option<NodeInfo>,
    pub cli: Option<CliInfo>,
    pub global_skills_dir: PathBuf,
    pub global_lock_path: PathBuf,
    /// Non-fatal notes for the diagnostics panel (why detection fell back,
    /// where the CLI was found, …).
    pub notes: Vec<String>,
}

impl RuntimeStatus {
    pub fn writes_supported(&self) -> bool {
        self.cli.as_ref().is_some_and(|c| c.writes_supported)
    }

    pub fn degraded(&self) -> bool {
        self.cli.is_none() || !self.writes_supported()
    }
}

/// Candidate node executables, best first: the app's PATH (terminal launch),
/// then locations reachable when launched from Finder/Dock with a minimal
/// PATH.
fn node_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(node) = find_in_path("node") {
        candidates.push(node);
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let exe = if cfg!(windows) { "node.exe" } else { "node" };
    let mut push = |p: PathBuf| {
        if !candidates.contains(&p) {
            candidates.push(p);
        }
    };
    // nvm: newest installed version.
    if let Some(dir) = newest_nvm_node(&home) {
        push(dir.join(exe));
    }
    // volta
    push(home.join(".volta/bin").join(exe));
    // fnm (macOS layout)
    if let Some(dir) = newest_fnm_node(&home) {
        push(dir.join(exe));
    }
    // homebrew (arm + intel) and system locations
    push(PathBuf::from("/opt/homebrew/bin").join(exe));
    push(PathBuf::from("/usr/local/bin").join(exe));
    push(PathBuf::from("/usr/bin").join(exe));
    // Windows defaults
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        push(PathBuf::from(pf).join("nodejs").join(exe));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        push(PathBuf::from(appdata).join("npm").join(exe));
    }
    candidates.retain(|p| p.is_file());
    candidates
}

fn newest_nvm_node(home: &Path) -> Option<PathBuf> {
    let versions = home.join(".nvm/versions/node");
    newest_versioned_dir(&versions)
}

fn newest_fnm_node(home: &Path) -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/fnm/node-versions")
    } else {
        home.join(".local/share/fnm/node-versions")
    };
    let dir = newest_versioned_dir(&base)?;
    Some(dir.join("installation"))
}

fn newest_versioned_dir(base: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(base).ok()?;
    let mut versions: Vec<(Version, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let ver = name.trim_start_matches('v').to_string();
            Version::parse(&ver).ok().map(|v| (v, e.path()))
        })
        .collect();
    versions.sort_by(|a, b| a.0.cmp(&b.0));
    versions.pop().map(|(_, p)| p)
}

fn find_in_path(exe: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(exe);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Candidate `skills` CLI bin shims, best first: a user override, PATH,
/// then npm-global layouts reachable without a shell PATH.
fn cli_bin_candidates(cli_override: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = cli_override {
        candidates.push(path.to_path_buf());
    }
    for name in ["skills", "skills.cmd", "skills.exe"] {
        if let Some(found) = find_in_path(name) {
            candidates.push(found);
            break;
        }
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let mut push = |p: PathBuf| {
        if !candidates.contains(&p) {
            candidates.push(p);
        }
    };
    // npm global prefix bin dir.
    if let Some(node) = find_in_path("node") {
        if let Some(prefix) = node.parent().and_then(|b| b.parent()) {
            push(prefix.join("bin").join("skills"));
        }
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let npm = PathBuf::from(appdata).join("npm");
        push(npm.join("skills.cmd"));
        push(npm.join("skills"));
    }
    push(home.join(".npm-global/bin/skills"));
    push(home.join(".local/bin/skills"));
    push(PathBuf::from("/opt/homebrew/bin/skills"));
    push(PathBuf::from("/usr/local/bin/skills"));
    candidates.retain(|p| p.exists());
    candidates
}

/// Map a `skills` bin shim to the package's `bin/cli.mjs` entry we can pass
/// to node. npm symlinks `bin/skills` → `<pkg>/bin/cli.mjs`, so following
/// symlinks usually lands there directly; `.cmd`/fallback layouts are
/// resolved from the npm directory structure.
fn resolve_cli_entry(bin: &Path) -> Option<PathBuf> {
    let real = bin
        .canonicalize()
        .unwrap_or_else(|_| bin.to_path_buf());
    if real.file_name().is_some_and(|n| n == "cli.mjs") {
        return Some(real);
    }
    // Windows .cmd shims live in <prefix>; unix shims in <prefix>/bin.
    let prefix = if real.file_name().is_some_and(|n| n == "skills.cmd") {
        real.parent()?.to_path_buf()
    } else {
        real.parent()?.parent()?.to_path_buf()
    };
    for rel in ["node_modules/skills/bin/cli.mjs", "lib/node_modules/skills/bin/cli.mjs"] {
        let candidate = prefix.join(rel);
        if candidate.is_file() {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    // A user override may point straight at any .mjs entry of the package.
    if real.extension().is_some_and(|e| e == "mjs") && real.is_file() {
        return Some(real);
    }
    None
}

/// Run `<node> --version` style probes: argv-array spawn, whitelisted env,
/// stdin null, short timeout.
async fn probe_version(node: &Path, entry: Option<&Path>, args: &[&str]) -> Option<String> {
    let mut cmd = tokio::process::Command::new(node);
    if let Some(entry) = entry {
        cmd.arg(entry);
    }
    cmd.args(args)
        .env_clear()
        .envs(build_cli_env(node.parent(), &super::skills_cli::ChildEnv::default()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), cmd.output()).await.ok()?.ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn parse_version(raw: &str) -> Option<Version> {
    let raw = raw.trim().trim_start_matches('v');
    Version::parse(raw).ok()
}

/// Full detection pass. Never panics, never writes to disk; every miss is
/// turned into a note.
pub async fn detect_runtime(cli_override: Option<&Path>) -> RuntimeStatus {
    let mut notes = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let global_skills_dir = super::scope::global_skills_dir_for(&home);
    let global_lock_path = super::scope::global_lock_path_for(
        &home,
        std::env::var("XDG_STATE_HOME").ok().as_deref(),
    );

    // Node first: without it nothing else can run.
    let mut node = None;
    for candidate in node_candidates() {
        if let Some(version) = probe_version(&candidate, None, &["--version"]).await {
            node = Some(NodeInfo {
                path: candidate,
                version,
            });
            break;
        }
    }
    let Some(node) = node else {
        notes.push(
            "Node.js not found. Install Node.js 22+ (https://nodejs.org) and the \
             Vercel skills CLI (`npm install -g skills`)."
                .into(),
        );
        return RuntimeStatus {
            node: None,
            cli: None,
            global_skills_dir,
            global_lock_path,
            notes,
        };
    };

    // Then the CLI: resolve each candidate shim to a runnable entry.
    let mut cli = None;
    for bin in cli_bin_candidates(cli_override) {
        let source = if cli_override.is_some() && bin == cli_override.unwrap() {
            CliSource::UserOverride
        } else if bin.parent().map(|p| in_path_dirs(p)).unwrap_or(false) {
            CliSource::Path
        } else {
            CliSource::CandidateDir
        };
        let Some(entry) = resolve_cli_entry(&bin) else {
            notes.push(format!(
                "found a skills binary at {} but could not resolve its cli.mjs entry",
                bin.display()
            ));
            continue;
        };
        let Some(version_raw) = probe_version(&node.path, Some(&entry), &["--version"]).await
        else {
            notes.push(format!(
                "skills entry {} did not respond to --version",
                entry.display()
            ));
            continue;
        };
        let parsed = parse_version(&version_raw);
        let writes_supported = parsed
            .as_ref()
            .is_some_and(|v| *v >= MIN_WRITE_VERSION);
        if !writes_supported {
            notes.push(format!(
                "skills CLI {version_raw} is older than {MIN_WRITE_VERSION}; write operations \
                 are disabled. Upgrade with `npm install -g skills@^{MIN_WRITE_VERSION}`."
            ));
        }
        cli = Some(CliInfo {
            runtime: CliRuntime {
                node_path: node.path.clone(),
                cli_entry: entry,
                version: version_raw,
                source,
            },
            version: parsed,
            writes_supported,
        });
        break;
    }
    if cli.is_none() && !notes.iter().any(|n| n.contains("skills")) {
        notes.push(
            "Vercel skills CLI not found. Install it with `npm install -g skills` and \
             restart the app, or set its path in Settings."
                .into(),
        );
    }

    RuntimeStatus {
        node: Some(node),
        cli,
        global_skills_dir,
        global_lock_path,
        notes,
    }
}

fn in_path_dirs(dir: &Path) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|p| p == dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_floor_is_1_7_0() {
        assert!(Version::new(1, 7, 0) >= MIN_WRITE_VERSION);
        assert!(Version::new(1, 7, 4) >= MIN_WRITE_VERSION);
        assert!(Version::new(1, 6, 9) < MIN_WRITE_VERSION);
        assert!(Version::new(1, 5, 1) < MIN_WRITE_VERSION);
    }

    #[test]
    fn parses_plain_and_v_prefixed_versions() {
        assert_eq!(parse_version("1.7.4").unwrap().to_string(), "1.7.4");
        assert_eq!(parse_version("v22.11.0").unwrap().to_string(), "22.11.0");
        assert!(parse_version("not-a-version").is_none());
        assert!(parse_version("").is_none());
    }

    #[tokio::test]
    async fn detects_node_on_this_machine() {
        // Test environments (dev machine and CI) always have node.
        let status = detect_runtime(None).await;
        let node = status.node.expect("node should be detectable");
        assert!(node.path.is_absolute());
        assert!(parse_version(&node.version).is_some(), "got {:?}", node.version);
    }

    #[tokio::test]
    async fn user_override_of_a_fake_cli_entry_is_probed_and_gated() {
        // An override pointing at a runnable .mjs entry wins over every other
        // candidate, so the result is machine-independent.
        let tmp = tempfile::tempdir().unwrap();
        for (script, writes) in [("console.log('1.7.4')", true), ("console.log('1.5.1')", false)] {
            let entry = tmp.path().join("cli.mjs");
            std::fs::write(&entry, script).unwrap();
            let status = detect_runtime(Some(&entry)).await;
            let cli = status.cli.as_ref().expect("override accepted");
            assert_eq!(cli.runtime.source, CliSource::UserOverride);
            assert_eq!(cli.runtime.cli_entry.canonicalize().unwrap(), entry.canonicalize().unwrap());
            assert_eq!(cli.writes_supported, writes);
            if !writes {
                assert!(status.notes.iter().any(|n| n.contains("older than")));
            }
        }
    }

    #[tokio::test]
    async fn resolves_a_fake_npm_layout_to_cli_entry() {
        // Simulate <prefix>/bin/skills -> <prefix>/lib/node_modules/skills/bin/cli.mjs
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp
            .path()
            .join("lib/node_modules/skills/bin/cli.mjs");
        std::fs::create_dir_all(pkg.parent().unwrap()).unwrap();
        std::fs::write(&pkg, "// fake entry\n").unwrap();
        let bin = tmp.path().join("bin/skills");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&pkg, &bin).unwrap();
        #[cfg(windows)]
        std::fs::copy(&pkg, &bin).unwrap();

        let entry = resolve_cli_entry(&bin).unwrap();
        assert_eq!(entry.canonicalize().unwrap(), pkg.canonicalize().unwrap());
    }

    #[tokio::test]
    async fn resolves_windows_cmd_shim_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("node_modules/skills/bin/cli.mjs");
        std::fs::create_dir_all(pkg.parent().unwrap()).unwrap();
        std::fs::write(&pkg, "// fake entry\n").unwrap();
        let shim = tmp.path().join("skills.cmd");
        std::fs::write(&shim, "@echo off\r\n").unwrap();
        // canonicalize would resolve the .cmd to itself on unix; emulate by
        // calling with the shim path directly.
        let entry = resolve_cli_entry(&shim).unwrap();
        assert!(entry.ends_with("node_modules/skills/bin/cli.mjs"));
    }

    #[tokio::test]
    async fn fake_cli_entry_reports_version_and_gating() {
        // A fake package whose --version prints 1.7.4 must be accepted for
        // writes; one printing 1.5.1 must be rejected for writes.
        let tmp = tempfile::tempdir().unwrap();
        let node = find_in_path(if cfg!(windows) { "node.exe" } else { "node" })
            .expect("node on test machine");
        for (script, writes) in [
            ("console.log('1.7.4')", true),
            ("console.log('1.5.1')", false),
        ] {
            let entry = tmp.path().join("cli.mjs");
            std::fs::write(&entry, script).unwrap();
            let version_raw =
                probe_version(&node, Some(&entry), &["--version"]).await.unwrap();
            let parsed = parse_version(&version_raw).unwrap();
            assert_eq!(
                parsed >= MIN_WRITE_VERSION,
                writes,
                "gating wrong for {version_raw}"
            );
        }
    }
}
