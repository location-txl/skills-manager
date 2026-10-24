//! IPC commands for the Vercel-CLI-backed Skills Manager.
//!
//! The surface is deliberately small and typed: the frontend never sends
//! argv, only named operations with validated parameters. Every write goes
//! through the shared queue and is followed by a disk re-scan; the CLI is
//! the only thing that ever touches the skills tree or its locks.

use crate::core::error::AppError;
use crate::core::marketplace;
use crate::core::scope::{self, ProjectRootCheck, Scope};
use crate::core::skill_catalog::{self, CatalogResult, CliSkillItem};
use crate::core::skill_document;
use crate::core::skills_cli::{self, CliCommandSpec, CliOutcome, CliRuntime};
use crate::core::state::{refresh_watch_roots, AppState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, State};

/// Scope as sent by the frontend.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ScopeArg {
    Global,
    Project { root: String },
}

impl ScopeArg {
    fn into_scope(self) -> Result<Scope, AppError> {
        match self {
            ScopeArg::Global => Ok(Scope::Global),
            ScopeArg::Project { root } => {
                let path = PathBuf::from(&root);
                match scope::validate_project_root(&path) {
                    ProjectRootCheck::Ok => {
                        let normalized = scope::normalize_project_root(&path)
                            .map_err(AppError::io)?;
                        Ok(Scope::Project(normalized))
                    }
                    ProjectRootCheck::NotAbsolute => {
                        Err(AppError::invalid_input("project root must be an absolute path"))
                    }
                    ProjectRootCheck::DoesNotExist => {
                        Err(AppError::not_found("project root does not exist"))
                    }
                    ProjectRootCheck::NotADirectory => {
                        Err(AppError::invalid_input("project root is not a directory"))
                    }
                    ProjectRootCheck::InsideLegacySkillsManager => Err(AppError::invalid_input(
                        "this folder belongs to the old Skills Manager; it is not a project",
                    )),
                }
            }
        }
    }
}

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn xdg_state_home() -> Option<String> {
    std::env::var("XDG_STATE_HOME").ok().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusDto {
    pub node_path: Option<String>,
    pub node_version: Option<String>,
    pub cli_path: Option<String>,
    pub cli_version: Option<String>,
    pub cli_source: Option<String>,
    pub writes_supported: bool,
    pub degraded: bool,
    pub global_skills_dir: String,
    pub global_lock_path: String,
    pub notes: Vec<String>,
}

impl From<crate::core::runtime_detector::RuntimeStatus> for RuntimeStatusDto {
    fn from(status: crate::core::runtime_detector::RuntimeStatus) -> Self {
        let degraded = status.degraded();
        let global_skills_dir = status.global_skills_dir.display().to_string();
        let global_lock_path = status.global_lock_path.display().to_string();
        let notes = status.notes.clone();
        let (node_path, node_version) = status
            .node
            .map(|n| (Some(n.path.display().to_string()), Some(n.version)))
            .unwrap_or((None, None));
        let (cli_path, cli_version, cli_source, writes_supported) = status
            .cli
            .map(|c| {
                (
                    Some(c.runtime.cli_entry.display().to_string()),
                    Some(c.runtime.version),
                    Some(format!("{:?}", c.runtime.source).to_lowercase()),
                    c.writes_supported,
                )
            })
            .unwrap_or((None, None, None, false));
        RuntimeStatusDto {
            node_path,
            node_version,
            cli_path,
            cli_version,
            cli_source,
            writes_supported,
            degraded,
            global_skills_dir,
            global_lock_path,
            notes,
        }
    }
}

#[tauri::command]
pub async fn get_runtime_status(state: State<'_, Arc<AppState>>) -> Result<RuntimeStatusDto, AppError> {
    let status = state.runtime_status().await;
    refresh_watch_roots(&state);
    Ok(status.into())
}

/// Set (or clear) a user-specified skills CLI path. `None` returns to
/// auto-detection.
#[tauri::command]
pub async fn set_cli_path(
    state: State<'_, Arc<AppState>>,
    path: Option<String>,
) -> Result<RuntimeStatusDto, AppError> {
    let override_path = path.map(PathBuf::from);
    state.set_cli_override(override_path);
    let status = state.runtime_status().await;
    Ok(status.into())
}

#[tauri::command]
pub async fn get_last_cli_error(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<String>, AppError> {
    Ok(state.last_cli_error())
}

// ---------------------------------------------------------------------------
// Listing / documents
// ---------------------------------------------------------------------------

/// Full snapshot for a scope: disk scan + lock + `skills ls --json` when the
/// CLI is available. CLI failures degrade to disk-only with a diagnostic.
#[tauri::command]
pub async fn list_skills(
    state: State<'_, Arc<AppState>>,
    scope_arg: ScopeArg,
) -> Result<CatalogResult, AppError> {
    let scope = scope_arg.into_scope()?;
    let home = home();
    let xdg = xdg_state_home();

    let disk = tokio::task::spawn_blocking({
        let scope = scope.clone();
        let home = home.clone();
        move || skill_catalog::build_catalog_from_disk(&scope, &home, xdg.as_deref())
    })
    .await
    .map_err(AppError::io)?;

    let runtime_status = state.runtime_status().await;
    let Some(cli) = runtime_status.cli.filter(|c| c.version.is_some()) else {
        return Ok(disk);
    };

    // `skills ls --json` for agent visibility. Read-only, short timeout.
    let mut args = vec!["ls".to_string(), "--json".to_string()];
    let mut cwd: Option<PathBuf> = None;
    match &scope {
        Scope::Global => args.push("-g".into()),
        Scope::Project(root) => cwd = Some(root.clone()),
    }
    let spec = CliCommandSpec::new(args)
        .with_timeout(Duration::from_secs(30))
        .with_cwd(cwd.unwrap_or_else(skills_cli::neutral_cwd));
    let outcome = skills_cli::run_cli_command(&cli.runtime, &spec, None).await?;
    if !outcome.success() {
        let message = format!(
            "skills ls failed (exit {:?}): {}",
            outcome.exit_code,
            crate::core::log_sanitize::sanitize(outcome.stderr.trim())
        );
        state.record_cli_error(message.clone());
        let mut degraded = disk;
        degraded.diagnostics.push(message);
        return Ok(degraded);
    }
    let items: Vec<CliSkillItem> = match skills_cli::parse_json_stdout(&outcome) {
        Ok(items) => items,
        Err(e) => {
            let mut degraded = disk;
            degraded.diagnostics.push(e.to_string());
            return Ok(degraded);
        }
    };
    Ok(skill_catalog::merge_cli_listing(disk, &items))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDocumentDto {
    pub markdown: String,
    pub truncated: bool,
}

#[tauri::command]
pub async fn read_skill_doc(
    scope_arg: ScopeArg,
    path: String,
) -> Result<SkillDocumentDto, AppError> {
    let scope = scope_arg.into_scope()?;
    let doc = tokio::task::spawn_blocking(move || {
        skill_document::read_skill_document(&scope, Path::new(&path), &home())
    })
    .await
    .map_err(AppError::io)?
    .map_err(|e| match e {
        skill_document::ReadDocError::OutsideScope => {
            AppError::invalid_input("path is outside the requested scope")
        }
        skill_document::ReadDocError::NotFound => AppError::not_found("SKILL.md not found"),
        skill_document::ReadDocError::TooLarge => AppError::invalid_input(
            "SKILL.md is too large to display",
        ),
        skill_document::ReadDocError::Io(err) => AppError::io(err),
    })?;
    Ok(SkillDocumentDto {
        markdown: doc.markdown,
        truncated: doc.truncated,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatusDto {
    pub id: String,
    pub name: String,
    pub detected: bool,
    pub supports_global: bool,
    pub global_dir: Option<String>,
    pub project_dir: String,
}

/// All agents of the controlled map with a `detected` flag (its skills dir
/// exists on disk). Purely informational — visibility per skill comes from
/// `list_skills`.
#[tauri::command]
pub async fn list_agent_status(
    scope_arg: ScopeArg,
) -> Result<Vec<AgentStatusDto>, AppError> {
    let scope = scope_arg.into_scope()?;
    let home = home();
    let agents = tokio::task::spawn_blocking(move || {
        crate::core::agents::AGENTS
            .iter()
            .map(|agent| {
                let (detected, global_dir) = match &scope {
                    Scope::Global => {
                        let dir = agent.global_skills_dir(&home);
                        (dir.as_ref().is_some_and(|d| d.is_dir()), dir)
                    }
                    Scope::Project(root) => {
                        let dir = agent.project_skills_dir(root);
                        (dir.is_dir(), None)
                    }
                };
                AgentStatusDto {
                    id: agent.key.to_string(),
                    name: agent.display_name.to_string(),
                    detected,
                    supports_global: agent.supports_global(),
                    global_dir: global_dir.map(|d| d.display().to_string()),
                    project_dir: agent.project_dir.to_string(),
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(AppError::io)?;
    Ok(agents)
}

// ---------------------------------------------------------------------------
// Marketplace
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn search_marketplace(
    query: String,
    owner: Option<String>,
) -> Result<Vec<marketplace::MarketSkill>, AppError> {
    marketplace::search(&query, owner.as_deref()).await
}

// ---------------------------------------------------------------------------
// Project session
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfoDto {
    pub root: String,
    pub name: String,
    pub has_lock: bool,
    pub skills_dir_exists: bool,
}

/// Open (or switch) the active project for this session and re-target the
/// watcher. Process state only — no registry, no persistence.
#[tauri::command]
pub async fn open_project(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<ProjectInfoDto, AppError> {
    let scope = ScopeArg::Project { root: path }.into_scope()?;
    let Scope::Project(root) = scope else {
        unreachable!();
    };
    let info = ProjectInfoDto {
        has_lock: root.join("skills-lock.json").is_file(),
        skills_dir_exists: root.join(".agents/skills").is_dir(),
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string()),
        root: root.display().to_string(),
    };
    state.set_current_project(Some(root));
    refresh_watch_roots(&state);
    Ok(info)
}

#[tauri::command]
pub async fn close_project(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    state.set_current_project(None);
    refresh_watch_roots(&state);
    Ok(())
}

/// Reveal a folder from the catalog in the system file manager. Only paths
/// that sit inside a scan root of the given scope are accepted.
#[tauri::command]
pub async fn reveal_skill_folder(
    scope_arg: ScopeArg,
    path: String,
) -> Result<(), AppError> {
    let scope = scope_arg.into_scope()?;
    let target = PathBuf::from(&path);
    let roots = skill_catalog::scan_roots_public(&scope, &home());
    if !roots.iter().any(|(root, _)| target.starts_with(root)) {
        return Err(AppError::invalid_input(
            "path is outside the requested scope",
        ));
    }
    if !target.is_dir() {
        return Err(AppError::not_found("folder not found"));
    }
    let open_status = crate::core::file_manager::open_dir(&target).map_err(AppError::io)?;
    if crate::core::file_manager::exit_is_failure(open_status.success()) {
        return Err(AppError::io(format!(
            "file manager exited with {}",
            open_status
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Write operations (the only path that changes anything on disk)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub source: String,
    pub scope: ScopeArg,
    /// Agent keys from the controlled map (never display names, never '*').
    pub agents: Vec<String>,
    /// Optional specific skill names within the source.
    pub skills: Option<Vec<String>>,
    /// Copy files into agent dirs instead of symlinking.
    pub copy: bool,
    /// Frontend-generated operation id for cancellation.
    pub op_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveRequest {
    /// Folder slug as shown by `list_skills`.
    pub name: String,
    pub scope: ScopeArg,
    /// None = remove for every agent; Some(keys) = scoped removal.
    pub agents: Option<Vec<String>>,
    pub op_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    /// Lock key (frontmatter name) — `skills update` matches on it.
    pub name: String,
    pub scope: ScopeArg,
    pub op_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationItem {
    pub name: String,
    /// installed | failed | skipped | removed | updated | unchanged
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub message: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub items: Vec<OperationItem>,
    pub cancelled: bool,
    pub timed_out: bool,
}

/// Anti-injection validation for anything that becomes one argv element.
/// A leading dash would be parsed as a flag; control characters never appear
/// in legitimate sources or names.
fn validate_argv_value(value: &str, what: &str) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::invalid_input(format!("{what} is empty")));
    }
    if value.starts_with('-') {
        return Err(AppError::invalid_input(format!(
            "{what} must not start with '-'"
        )));
    }
    if value.len() > 2048 {
        return Err(AppError::invalid_input(format!("{what} is too long")));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(AppError::invalid_input(format!(
            "{what} contains control characters"
        )));
    }
    Ok(())
}

/// Translate agent keys, rejecting anything outside the controlled map and
/// agents without global support for global installs.
fn validate_agent_keys(agents: &[String], global: bool) -> Result<Vec<String>, AppError> {
    if agents.is_empty() {
        return Err(AppError::invalid_input("select at least one agent"));
    }
    let mut keys = Vec::with_capacity(agents.len());
    for key in agents {
        let agent = crate::core::agents::by_key(key).ok_or_else(|| {
            AppError::invalid_input(format!("unknown agent: {key}"))
        })?;
        if global && !agent.supports_global() {
            return Err(AppError::invalid_input(format!(
                "{} does not support global installation",
                agent.display_name
            )));
        }
        keys.push(agent.key.to_string());
    }
    Ok(keys)
}

/// Well-known sources (any http(s) URL that is not github/gitlab or one of
/// their artifact hosts) reject `--json` in the CLI, so they run without it.
fn is_well_known_source(source: &str) -> bool {
    let Ok(url) = url::Url::parse(source) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    !matches!(
        host,
        "github.com" | "gitlab.com" | "raw.githubusercontent.com" | "codeload.github.com"
    )
}

fn sanitize_output(text: &str) -> String {
    let clean = crate::core::log_sanitize::sanitize(text);
    if clean.len() > 8000 {
        let mut end = 8000;
        while end > 0 && !clean.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n… (truncated)", &clean[..end])
    } else {
        clean
    }
}

fn snapshot_names(scope: &Scope) -> BTreeSet<String> {
    skill_catalog::build_catalog_from_disk(scope, &home(), xdg_state_home().as_deref())
        .skills
        .into_iter()
        .map(|s| s.name)
        .collect()
}

async fn cli_for_writes(state: &State<'_, Arc<AppState>>) -> Result<CliRuntime, AppError> {
    let status = state.runtime_status().await;
    let Some(cli) = status.cli else {
        return Err(AppError::invalid_input(
            "Vercel skills CLI was not found. Install it with `npm install -g skills` and retry.",
        ));
    };
    if !cli.writes_supported {
        return Err(AppError::invalid_input(format!(
            "skills CLI {} is older than 1.7.0; write operations need skills@^1.7.0. \
             Upgrade with `npm install -g skills@^1.7.0`.",
            cli.runtime.version
        )));
    }
    Ok(cli.runtime)
}

fn result_from_outcome(
    outcome: &CliOutcome,
    message: String,
    items: Vec<OperationItem>,
) -> OperationResult {
    OperationResult {
        success: !outcome.timed_out
            && !outcome.cancelled
            && outcome.exit_code == Some(0)
            && items.iter().all(|i| i.status != "failed"),
        exit_code: outcome.exit_code,
        message,
        stdout: Some(sanitize_output(&outcome.stdout)).filter(|s| !s.trim().is_empty()),
        stderr: Some(sanitize_output(&outcome.stderr)).filter(|s| !s.trim().is_empty()),
        items,
        cancelled: outcome.cancelled,
        timed_out: outcome.timed_out,
    }
}

/// The CLI's `add --json` result items.
#[derive(Debug, Deserialize)]
struct AddJsonItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[tauri::command]
pub async fn install_skill(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    request: InstallRequest,
) -> Result<OperationResult, AppError> {
    validate_argv_value(&request.source, "source")?;
    let scope = request.scope.clone().into_scope()?;
    let global = scope.is_global();
    let agent_keys = validate_agent_keys(&request.agents, global)?;
    if let Some(skills) = &request.skills {
        for skill in skills {
            validate_argv_value(skill, "skill name")?;
        }
    }

    let cli = cli_for_writes(&state).await?;
    // Hold the write queue for the whole operation.
    let _guard = state.write_queue.lock().await;
    let cancel = state.register_cancel(&request.op_id);

    let before = snapshot_names(&scope);

    let use_json = !is_well_known_source(&request.source);
    let mut args = vec!["add".to_string(), request.source.clone()];
    args.push("-a".into());
    args.extend(agent_keys.iter().cloned());
    if let Some(skills) = &request.skills {
        args.push("-s".into());
        args.extend(skills.iter().cloned());
    }
    args.push("-y".into());
    if global {
        args.push("-g".into());
    }
    if use_json {
        args.push("--json".into());
    }
    if request.copy {
        args.push("--copy".into());
    }

    let cwd = match &scope {
        Scope::Global => skills_cli::neutral_cwd(),
        Scope::Project(root) => root.clone(),
    };
    let spec = CliCommandSpec::new(args)
        .with_timeout(Duration::from_secs(600))
        .with_cwd(cwd);
    let outcome = skills_cli::run_cli_command(&cli, &spec, Some(&cancel)).await;
    state.finish_operation(&request.op_id);
    let outcome = outcome?;

    // Per-item results when JSON is available; disk-verified diff otherwise.
    let items = if use_json {
        match skills_cli::parse_json_stdout::<Vec<AddJsonItem>>(&outcome) {
            Ok(parsed) => parsed
                .into_iter()
                .map(|item| OperationItem {
                    detail: item.error.or(item.reason),
                    status: if item.status == "installed" {
                        "installed".to_string()
                    } else {
                        item.status
                    },
                    name: item.name,
                })
                .collect(),
            Err(e) => vec![OperationItem {
                name: request.source.clone(),
                status: "failed".to_string(),
                detail: Some(e.to_string()),
            }],
        }
    } else {
        Vec::new()
    };

    // Verify against disk regardless: the CLI exit code is not the truth.
    let after = snapshot_names(&scope);
    let added: Vec<String> = after.difference(&before).cloned().collect();
    if items.is_empty() {
        // Non-JSON mode (well-known sources): report what actually changed.
        let mut verification = Vec::new();
        for name in &added {
            verification.push(OperationItem {
                name: name.clone(),
                status: "installed".into(),
                detail: None,
            });
        }
        let message = if outcome.success() {
            if added.is_empty() {
                "The CLI reported success, but no new skills appeared on disk. \
                 The refreshed list shows the current state."
                    .to_string()
            } else {
                format!("Installed {} skill(s): {}", added.len(), added.join(", "))
            }
        } else {
            "Installation failed. The refreshed list shows the current state.".to_string()
        };
        let mut result = result_from_outcome(&outcome, message, verification);
        if !outcome.success() {
            result.success = false;
        }
        refresh_watch_roots(&state);
        let _ = app.emit(crate::core::change_watcher::SKILLS_CHANGED_EVENT, ());
        return Ok(result);
    }

    let failed = items.iter().any(|i| i.status == "failed" || i.status == "skipped");
    let message = if failed {
        "Some skills could not be installed. See the per-skill results; the \
         refreshed list shows the current state."
            .to_string()
    } else {
        format!("Installed {} skill(s).", items.len())
    };
    let mut result = result_from_outcome(&outcome, message, items);
    if outcome.success() && !failed {
        result.success = true;
    }
    refresh_watch_roots(&state);
    let _ = app.emit(crate::core::change_watcher::SKILLS_CHANGED_EVENT, ());
    Ok(result)
}

#[tauri::command]
pub async fn remove_skill(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    request: RemoveRequest,
) -> Result<OperationResult, AppError> {
    validate_argv_value(&request.name, "skill name")?;
    let scope = request.scope.clone().into_scope()?;
    let global = scope.is_global();
    if let Some(agents) = &request.agents {
        validate_agent_keys(agents, global)?;
    }

    let cli = cli_for_writes(&state).await?;
    let _guard = state.write_queue.lock().await;
    let cancel = state.register_cancel(&request.op_id);

    let before = snapshot_names(&scope);

    let mut args = vec!["remove".to_string(), request.name.clone(), "-y".into()];
    if global {
        args.push("-g".into());
    }
    if let Some(agents) = &request.agents {
        args.push("-a".into());
        args.extend(agents.iter().cloned());
    }
    let cwd = match &scope {
        Scope::Global => skills_cli::neutral_cwd(),
        Scope::Project(root) => root.clone(),
    };
    let spec = CliCommandSpec::new(args)
        .with_timeout(Duration::from_secs(120))
        .with_cwd(cwd);
    let outcome = skills_cli::run_cli_command(&cli, &spec, Some(&cancel)).await;
    state.finish_operation(&request.op_id);
    let outcome = outcome?;

    // Verify: full removal must be gone from the snapshot; agent-scoped
    // removal keeps the skill (shared visibility rules apply).
    let after = snapshot_names(&scope);
    let gone = !after.contains(&request.name) && !before.is_empty();
    let message = if request.agents.is_some() {
        format!(
            "Removed the deployment for the selected agent(s). Note: agents that read \
             the shared .agents/skills directory may still see this skill."
        )
    } else if gone || before.is_empty() {
        format!("Removed {}.", request.name)
    } else if outcome.success() {
        format!(
            "The CLI reported success, but {} is still present on disk. The refreshed \
             list shows the current state.",
            request.name
        )
    } else {
        format!("Removal of {} failed.", request.name)
    };

    let mut items = vec![OperationItem {
        name: request.name.clone(),
        status: if outcome.success() { "removed" } else { "failed" }.into(),
        detail: None,
    }];
    if !request.agents.is_some() && outcome.success() && !gone && !before.is_empty() {
        items[0].status = "unchanged".into();
        items[0].detail = Some("still present after removal".into());
    }

    refresh_watch_roots(&state);
    let _ = app.emit(crate::core::change_watcher::SKILLS_CHANGED_EVENT, ());
    let mut result = result_from_outcome(&outcome, message, items);
    if request.agents.is_some() {
        // Agent-scoped removal succeeds when the CLI exits 0; presence of the
        // skill itself is expected.
        result.success = outcome.exit_code == Some(0) && !outcome.timed_out && !outcome.cancelled;
        if result.success {
            result.items[0].status = "removed".into();
            result.items[0].detail = None;
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn update_skill(
    state: State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
    request: UpdateRequest,
) -> Result<OperationResult, AppError> {
    // `skills update` matches the lock key (frontmatter name), case
    // insensitive — never the folder slug.
    validate_argv_value(&request.name, "skill name")?;
    let scope = request.scope.clone().into_scope()?;
    let global = scope.is_global();

    let cli = cli_for_writes(&state).await?;
    let _guard = state.write_queue.lock().await;
    let cancel = state.register_cancel(&request.op_id);

    let mut args = vec![
        "update".to_string(),
        request.name.clone(),
        "-y".into(),
    ];
    args.push(if global { "-g".into() } else { "-p".into() });
    let cwd = match &scope {
        Scope::Global => skills_cli::neutral_cwd(),
        Scope::Project(root) => root.clone(),
    };
    let spec = CliCommandSpec::new(args)
        .with_timeout(Duration::from_secs(600))
        .with_cwd(cwd);
    let outcome = skills_cli::run_cli_command(&cli, &spec, Some(&cancel)).await;
    state.finish_operation(&request.op_id);
    let outcome = outcome?;

    let message = if outcome.cancelled {
        "Update cancelled. Disk state may have partially changed; the \
         refreshed list shows the current state."
            .to_string()
    } else if outcome.timed_out {
        "Update timed out and was stopped. Disk state may have partially \
         changed; the refreshed list shows the current state."
            .to_string()
    } else if outcome.success() {
        format!("Update of {} finished.", request.name)
    } else {
        format!("Update of {} failed.", request.name)
    };
    let items = vec![OperationItem {
        name: request.name.clone(),
        status: if outcome.success() { "updated" } else { "failed" }.into(),
        detail: None,
    }];

    refresh_watch_roots(&state);
    let _ = app.emit(crate::core::change_watcher::SKILLS_CHANGED_EVENT, ());
    Ok(result_from_outcome(&outcome, message, items))
}

#[tauri::command]
pub async fn cancel_operation(
    state: State<'_, Arc<AppState>>,
    op_id: String,
) -> Result<bool, AppError> {
    Ok(state.cancel_operation(&op_id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_values_reject_flags_and_junk() {
        assert!(validate_argv_value("vercel-labs/agent-skills", "source").is_ok());
        assert!(validate_argv_value("./local/path", "source").is_ok());
        assert!(validate_argv_value("https://example.com/x", "source").is_ok());
        assert!(validate_argv_value("", "source").is_err());
        assert!(validate_argv_value("-g", "source").is_err());
        assert!(validate_argv_value("--json", "source").is_err());
        assert!(validate_argv_value(&"x".repeat(3000), "source").is_err());
        assert!(validate_argv_value("bad\u{1b}[31m", "source").is_err());
    }

    #[test]
    fn agent_keys_validate_against_the_map() {
        assert!(validate_agent_keys(&["claude-code".into()], true).is_ok());
        // eve/promptscript have no global scope.
        assert!(validate_agent_keys(&["eve".into()], true).is_err());
        assert!(validate_agent_keys(&["promptscript".into()], true).is_err());
        // Project scope allows eve.
        assert!(validate_agent_keys(&["eve".into()], false).is_ok());
        // Display names are rejected — only keys pass.
        assert!(validate_agent_keys(&["Claude Code".into()], true).is_err());
        assert!(validate_agent_keys(&[], true).is_err());
        assert!(validate_agent_keys(&["*".into()], true).is_err());
    }

    #[test]
    fn well_known_detection_matches_the_source_parser() {
        assert!(is_well_known_source("https://skills.sh/p/some-pack"));
        assert!(is_well_known_source("https://open.feishu.cn/.well-known/skills/x/SKILL.md"));
        assert!(is_well_known_source("https://example.com/anything"));
        assert!(!is_well_known_source("https://github.com/owner/repo"));
        assert!(!is_well_known_source("https://github.com/owner/repo/tree/main/skills/x"));
        assert!(!is_well_known_source("https://gitlab.com/owner/repo"));
        assert!(!is_well_known_source("https://raw.githubusercontent.com/o/r/main/SKILL.md"));
        assert!(!is_well_known_source("vercel-labs/agent-skills"));
        assert!(!is_well_known_source("./local"));
        assert!(!is_well_known_source("git@github.com:owner/repo.git"));
    }

    #[test]
    fn output_is_sanitized_and_capped() {
        assert!(sanitize_output("clean").contains("clean"));
        assert!(sanitize_output(&"x".repeat(20_000)).contains("(truncated)"));
    }

    #[test]
    fn add_json_items_parse() {
        let json = r#"[{"name":"web-design","status":"installed","source":"a/b","path":"/x","scope":"global","agents":["Claude Code"],"mode":"symlink"},
                       {"name":"missing","status":"skipped","reason":"No matching skill found in source"},
                       {"name":"bad","status":"failed","error":"clone failed"}]"#;
        let items: Vec<AddJsonItem> = serde_json::from_str(json).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].status, "skipped");
        assert!(items[2].error.is_some());
    }
}
