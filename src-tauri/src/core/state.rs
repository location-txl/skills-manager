//! Application state for the Vercel-CLI-backed backend.
//!
//! Process-scoped and intentionally tiny: a runtime-detection cache, the
//! write-operation queue, a cancel registry, the currently open project and
//! the watcher handle. Nothing here is a database; nothing survives restart
//! except what the frontend keeps as pure UI preferences.

use super::change_watcher::WatchRootsHandle;
use super::runtime_detector::{detect_runtime, RuntimeStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::watch;

pub struct AppState {
    runtime: RwLock<Option<RuntimeStatus>>,
    cli_override: RwLock<Option<PathBuf>>,
    /// Serializes every CLI write operation (install/remove/update).
    pub write_queue: tokio::sync::Mutex<()>,
    /// Last CLI failure surfaced on the diagnostics page (memory only).
    last_cli_error: RwLock<Option<String>>,
    /// The project directory the user opened this session (if any).
    open_project: RwLock<Option<PathBuf>>,
    pub watch_roots: WatchRootsHandle,
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        Self {
            runtime: RwLock::new(None),
            cli_override: RwLock::new(None),
            write_queue: tokio::sync::Mutex::new(()),
            last_cli_error: RwLock::new(None),
            open_project: RwLock::new(None),
            watch_roots: WatchRootsHandle::default(),
            cancels: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_cli_override(&self, path: Option<PathBuf>) {
        *self.cli_override.write().unwrap() = path;
        self.invalidate_runtime();
    }

    pub fn cli_override(&self) -> Option<PathBuf> {
        self.cli_override.read().unwrap().clone()
    }

    /// Cached status, refreshing on first use or after an invalidation.
    pub async fn runtime_status(&self) -> RuntimeStatus {
        if let Some(status) = self.runtime.read().unwrap().clone() {
            return status;
        }
        let override_path = self.cli_override();
        let status =
            tauri::async_runtime::spawn_blocking(move || {
                // detect_runtime is async but only spawns tiny processes; run
                // it on the blocking pool via a current-thread runtime so the
                // async runtime isn't blocked.
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(detect_runtime(override_path.as_deref()))
            })
            .await
            .unwrap_or_default();
        *self.runtime.write().unwrap() = Some(status.clone());
        status
    }

    pub fn invalidate_runtime(&self) {
        *self.runtime.write().unwrap() = None;
    }

    pub fn record_cli_error(&self, message: String) {
        *self.last_cli_error.write().unwrap() = Some(message);
    }

    pub fn last_cli_error(&self) -> Option<String> {
        self.last_cli_error.read().unwrap().clone()
    }

    pub fn current_project(&self) -> Option<PathBuf> {
        self.open_project.read().unwrap().clone()
    }

    pub fn set_current_project(&self, root: Option<PathBuf>) {
        *self.open_project.write().unwrap() = root;
    }

    /// Register a cancel channel for an operation id; returns the handle the
    /// executor polls.
    pub fn register_cancel(&self, op_id: &str) -> super::skills_cli::CancelHandle {
        let (tx, handle) = super::skills_cli::CancelHandle::new();
        self.cancels
            .lock()
            .unwrap()
            .insert(op_id.to_string(), tx);
        handle
    }

    pub fn finish_operation(&self, op_id: &str) {
        self.cancels.lock().unwrap().remove(op_id);
    }

    pub fn cancel_operation(&self, op_id: &str) -> bool {
        match self.cancels.lock().unwrap().remove(op_id) {
            Some(tx) => tx.send(true).is_ok(),
            None => false,
        }
    }
}

/// Recompute watcher roots from the current world: the global CLI territory,
/// plus the open project when there is one.
pub fn refresh_watch_roots(state: &AppState) {
    let mut roots = Vec::new();
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    // ~/.agents covers both the shared skills dir and the default lock file.
    let agents_dir = home.join(".agents");
    if agents_dir.is_dir() {
        roots.push(agents_dir.clone());
    }
    // A redirected global lock lives under $XDG_STATE_HOME/skills.
    if let Some(xdg) = std::env::var("XDG_STATE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
    {
        let lock_dir = PathBuf::from(xdg).join("skills");
        if lock_dir.is_dir() {
            roots.push(lock_dir);
        }
    }
    // Agent global dirs (symlinks land there outside ~/.agents).
    for agent in super::agents::AGENTS {
        if let Some(dir) = agent.global_skills_dir(&home) {
            if dir.is_dir() && !roots.contains(&dir) && !dir.starts_with(&agents_dir) {
                roots.push(dir);
            }
        }
    }
    // The open project: its canonical dir, lock file, and agent project dirs.
    if let Some(project) = state.current_project() {
        let shared = super::scope::project_skills_dir_for(&project);
        if shared.is_dir() {
            roots.push(shared);
        }
        let lock = project.join("skills-lock.json");
        if lock.is_file() {
            roots.push(lock);
        }
        for agent in super::agents::AGENTS {
            if agent.project_dir == ".agents/skills" {
                continue;
            }
            let dir = agent.project_skills_dir(&project);
            if dir.is_dir() && !roots.contains(&dir) {
                roots.push(dir);
            }
        }
    }
    state.watch_roots.set(roots);
}

/// Shared handle managed by Tauri.
pub type SharedAppState = Arc<AppState>;
