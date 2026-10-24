//! Contract tests against the real Vercel `skills` CLI (skills@1.7.4),
//! installed on demand into an isolated npm prefix.
//!
//! Every test runs with its own throwaway HOME (and XDG_STATE_HOME where the
//! case calls for it) injected into the child process environment — the
//! developer's real `~/.agents` is never touched. Sources are local
//! directories, so the flows don't depend on network reachability beyond the
//! one-time `npm install` of the CLI itself. If npm or the network is not
//! available, the whole module skips with a message.

use super::skills_cli::{
    build_cli_env, run_cli_command, ChildEnv, CliCommandSpec, CliRuntime, CliSource,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

const CLI_VERSION: &str = "1.7.4";

struct ContractCli {
    runtime: CliRuntime,
}

static CONTRACT_CLI: OnceLock<Option<ContractCli>> = OnceLock::new();
static SERIALIZE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn npm() -> Option<PathBuf> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(npm);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn node() -> Option<PathBuf> {
    let node = if cfg!(windows) { "node.exe" } else { "node" };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(node);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Install skills@CLI_VERSION into a temp prefix once per test process.
fn contract_cli() -> Option<&'static ContractCli> {
    CONTRACT_CLI
        .get_or_init(|| {
            let (npm, node) = (npm()?, node()?);
            let prefix = std::env::temp_dir().join(format!(
                "skills-manager-contract-{}",
                std::process::id()
            ));
            let entry = prefix.join("node_modules/skills/bin/cli.mjs");
            if !entry.is_file() {
                let status = Command::new(&npm)
                    .args([
                        "install",
                        "--prefix",
                        prefix.to_str()?,
                        "--no-audit",
                        "--no-fund",
                        &format!("skills@{CLI_VERSION}"),
                    ])
                    .stdin(std::process::Stdio::null())
                    .output()
                    .ok()?;
                if !status.status.success() {
                    eprintln!(
                        "contract tests: npm install failed: {}",
                        String::from_utf8_lossy(&status.stderr)
                    );
                    return None;
                }
            }
            Some(ContractCli {
                runtime: CliRuntime {
                    node_path: node,
                    cli_entry: entry,
                    version: CLI_VERSION.into(),
                    source: CliSource::CandidateDir,
                },
            })
        })
        .as_ref()
}

fn isolated_env(home: &Path) -> ChildEnv {
    ChildEnv {
        home: Some(home.to_path_buf()),
        xdg_state_home: Some(None),
    }
}

/// A local skill source directory the CLI can install from.
fn make_local_source(tag: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join("my-contract-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: my-contract-skill\ndescription: contract test skill {tag}\n---\n\n# hi\n"
        ),
    )
    .unwrap();
    dir
}

async fn run(
    cli: &ContractCli,
    args: &[&str],
    cwd: Option<&Path>,
    env: &ChildEnv,
) -> super::skills_cli::CliOutcome {
    let mut spec = CliCommandSpec::new(args.iter().map(|s| s.to_string()).collect())
        .with_timeout(Duration::from_secs(120))
        .with_env(env.clone());
    spec.cwd = cwd.map(|p| p.to_path_buf());
    run_cli_command(&cli.runtime, &spec, None).await.unwrap()
}

fn global_home() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[tokio::test]
async fn version_reports_anchored_semver() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let home = global_home();
    let out = run(cli, &["--version"], None, &isolated_env(home.path())).await;
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.stdout.trim(), CLI_VERSION);
}

#[tokio::test]
async fn empty_home_lists_nothing_and_writes_nothing() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let home = global_home();
    let out = run(cli, &["ls", "-g", "--json"], None, &isolated_env(home.path())).await;
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(out.stdout.trim(), "[]");
    // The CLI must not have created anything beyond its own directories.
    assert!(!home.path().join(".skills-manager").exists());
}

#[tokio::test]
async fn global_install_remove_and_lock_lifecycle() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let _serial = SERIALIZE.lock().unwrap();
    let home = global_home();
    let env = isolated_env(home.path());
    let source = make_local_source("global");
    let source_arg = source.path().display().to_string();

    // Two non-universal agents force symlink mode: canonical shared dir +
    // one link per agent. (A single agent would copy straight into its dir.)
    let out = run(
        cli,
        &[
            "add", &source_arg, "-g", "-a", "claude-code", "-a", "windsurf", "-y", "--json",
        ],
        None,
        &env,
    )
    .await;
    assert_eq!(
        out.exit_code,
        Some(0),
        "add failed: {} {}",
        out.stdout,
        out.stderr
    );
    let items: serde_json::Value =
        serde_json::from_str(out.stdout.trim()).expect("add --json output");
    let first = &items[0];
    assert_eq!(first["status"], "installed", "got {items}");
    assert_eq!(first["name"], "my-contract-skill");
    assert_eq!(first["scope"], "global");
    assert_eq!(first["mode"], "symlink");

    // Disk: canonical dir + two agent symlinks + v3 lock entry.
    let skill_dir = home.path().join(".agents/skills/my-contract-skill");
    assert!(skill_dir.join("SKILL.md").is_file());
    for link in [
        home.path().join(".claude/skills/my-contract-skill"),
        home.path().join(".codeium/windsurf/skills/my-contract-skill"),
    ] {
        #[cfg(unix)]
        assert!(
            link.symlink_metadata().unwrap().file_type().is_symlink(),
            "expected symlink at {}",
            link.display()
        );
        #[cfg(windows)]
        assert!(link.is_dir(), "expected link target at {}", link.display());
    }
    let lock_path = home.path().join(".agents/.skill-lock.json");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(lock["version"], 3);
    assert!(lock["skills"]["my-contract-skill"].is_object());

    // The catalog reads the same world without any CLI help.
    let catalog = super::skill_catalog::build_catalog_from_disk(
        &super::scope::Scope::Global,
        home.path(),
        None,
    );
    assert_eq!(catalog.skills.len(), 1);
    let view = &catalog.skills[0];
    assert_eq!(view.status, "installed");
    assert!(view.is_tracked_by_vercel);
    assert!(view
        .agents
        .iter()
        .any(|a| a.id == "claude-code" && a.exposure == "symlink"));
    assert!(view
        .agents
        .iter()
        .any(|a| a.id == "windsurf" && a.exposure == "symlink"));

    // ls -g --json sees it with display names and source fields.
    let out = run(cli, &["ls", "-g", "--json"], None, &env).await;
    let listed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(listed[0]["name"], "my-contract-skill");
    let listed_agents = listed[0]["agents"].as_array().unwrap();
    assert!(listed_agents.iter().any(|a| a == "Claude Code"));
    assert!(listed_agents.iter().any(|a| a == "Windsurf"));
    assert_eq!(listed[0]["sourceType"], "local");

    // Remove globally: files, links and lock entry all go away.
    let out = run(cli, &["remove", "my-contract-skill", "-g", "-y"], None, &env).await;
    assert_eq!(out.exit_code, Some(0), "remove failed: {}", out.stderr);
    assert!(!skill_dir.exists());
    assert!(!home.path().join(".claude/skills/my-contract-skill").exists());
    assert!(
        !home
            .path()
            .join(".codeium/windsurf/skills/my-contract-skill")
            .exists()
    );
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert!(lock["skills"].as_object().unwrap().is_empty());
}

#[tokio::test]
async fn project_install_uses_cwd_and_writes_v1_lock() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let _serial = SERIALIZE.lock().unwrap();
    let home = global_home();
    let project = tempfile::tempdir().unwrap();
    let env = isolated_env(home.path());
    let source = make_local_source("project");
    let source_arg = source.path().display().to_string();

    // Project install for claude-code only: single target dir forces copy
    // mode straight into <project>/.claude/skills (claude-code is the one
    // agent whose project dir is created by default).
    let out = run(
        cli,
        &["add", &source_arg, "-a", "claude-code", "-y", "--json"],
        Some(project.path()),
        &env,
    )
    .await;
    assert_eq!(out.exit_code, Some(0), "add failed: {}", out.stderr);
    let items: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(items[0]["status"], "installed");
    assert_eq!(items[0]["scope"], "project");
    assert_eq!(items[0]["mode"], "copy");

    // Files land in the project, never in the process cwd or the GUI home.
    assert!(project
        .path()
        .join(".claude/skills/my-contract-skill/SKILL.md")
        .is_file());
    assert!(!home.path().join(".agents/skills/my-contract-skill").exists());
    let lock_path = project.path().join("skills-lock.json");
    let lock: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(lock["version"], 1);

    // Project-scope catalog sees it; the ls (cwd=project) agrees.
    let catalog = super::skill_catalog::build_catalog_from_disk(
        &super::scope::Scope::Project(project.path().to_path_buf()),
        home.path(),
        None,
    );
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].scope, "project");
    assert!(catalog.skills[0]
        .agents
        .iter()
        .any(|a| a.id == "claude-code"));
    let out = run(cli, &["ls", "--json"], Some(project.path()), &env).await;
    let listed: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(listed[0]["name"], "my-contract-skill");
    assert_eq!(listed[0]["scope"], "project");
}

#[tokio::test]
async fn update_matches_lock_key_and_stays_installable() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let _serial = SERIALIZE.lock().unwrap();
    let home = global_home();
    let env = isolated_env(home.path());
    let source = make_local_source("update");
    let source_arg = source.path().display().to_string();
    let out = run(
        cli,
        &[
            "add", &source_arg, "-g", "-a", "claude-code", "-a", "windsurf", "-y", "--json",
        ],
        None,
        &env,
    )
    .await;
    assert_eq!(out.exit_code, Some(0), "setup add failed: {}", out.stderr);

    // update by lock key, non-interactive.
    let out = run(cli, &["update", "my-contract-skill", "-g", "-y"], None, &env).await;
    assert_eq!(
        out.exit_code,
        Some(0),
        "update failed: {} {}",
        out.stdout,
        out.stderr
    );
    assert!(home
        .path()
        .join(".agents/skills/my-contract-skill/SKILL.md")
        .is_file());
}

#[tokio::test]
async fn xdg_state_home_redirects_the_global_lock() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let _serial = SERIALIZE.lock().unwrap();
    let home = global_home();
    let xdg = tempfile::tempdir().unwrap();
    let env = ChildEnv {
        home: Some(home.path().to_path_buf()),
        xdg_state_home: Some(Some(xdg.path().display().to_string())),
    };
    let source = make_local_source("xdg");
    let source_arg = source.path().display().to_string();
    let out = run(
        cli,
        &[
            "add", &source_arg, "-g", "-a", "claude-code", "-a", "windsurf", "-y", "--json",
        ],
        None,
        &env,
    )
    .await;
    assert_eq!(out.exit_code, Some(0), "add failed: {}", out.stderr);

    // The lock lives under XDG_STATE_HOME, not ~/.agents.
    let xdg_lock = xdg.path().join("skills/.skill-lock.json");
    assert!(xdg_lock.is_file(), "expected lock at {xdg_lock:?}");
    assert!(!home.path().join(".agents/.skill-lock.json").exists());

    // The catalog follows the same rule when told about XDG.
    let catalog = super::skill_catalog::build_catalog_from_disk(
        &super::scope::Scope::Global,
        home.path(),
        Some(xdg.path().to_str().unwrap()),
    );
    assert_eq!(catalog.skills.len(), 1);
    assert!(catalog.skills[0].is_tracked_by_vercel);
}

#[tokio::test]
async fn json_mode_requires_yes_and_list_json_is_rejected() {
    let Some(cli) = contract_cli() else {
        eprintln!("contract tests: npm/network unavailable, skipping");
        return;
    };
    let home = global_home();
    let env = isolated_env(home.path());

    // --json without -y is refused before anything happens.
    let out = run(
        cli,
        &["add", "vercel-labs/agent-skills", "-g", "--json"],
        None,
        &env,
    )
    .await;
    assert_eq!(out.exit_code, Some(1));
    assert!(out.stderr.contains("--yes") || out.stdout.contains("--yes"));

    // --json with --list is refused outright (with -y so the earlier
    // "--json requires --yes" check doesn't preempt it).
    let out = run(
        cli,
        &["add", "vercel-labs/agent-skills", "--list", "--json", "-y"],
        None,
        &env,
    )
    .await;
    assert_eq!(out.exit_code, Some(1));
    let body = format!("{}{}", out.stdout, out.stderr);
    assert!(
        body.contains("--json flag cannot be combined with --list"),
        "unexpected body: {body}"
    );
}

#[test]
fn contract_env_matches_the_executor_whitelist() {
    // The contract runner must inject HOME exactly like the app executor
    // does, or the tests would prove nothing about production behavior.
    let home = tempfile::tempdir().unwrap();
    let env = isolated_env(home.path());
    let built = build_cli_env(None, &env);
    assert_eq!(
        built.get("HOME").map(String::as_str),
        Some(home.path().to_str().unwrap())
    );
    assert!(!built.contains_key("XDG_STATE_HOME"));
    assert_eq!(built.get("DISABLE_TELEMETRY").map(String::as_str), Some("1"));
}
