//! File-change watcher for the Vercel CLI's territory.
//!
//! Watches the global shared skills dir, the global lock file's directory,
//! and (while a project is open) the project's skills/lock locations. Any
//! relevant change — including ones made by the CLI we spawned or by an
//! external terminal — emits `skills-changed` so the frontend re-reads the
//! facts from disk. Read-only: this module never writes, never mutates, and
//! deliberately keeps no cache of what it saw.
//!
//! Debounce coalesces bursts; a periodic watch-set resync (3s) recovers
//! watches on directories that were deleted and recreated, and picks up
//! watch-root changes (e.g. the user opens a different project).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{Config, Event, RecursiveMode, Watcher};
use tauri::Emitter;

pub const SKILLS_CHANGED_EVENT: &str = "skills-changed";

const RESCAN_INTERVAL: Duration = Duration::from_secs(3);
const EMIT_DEBOUNCE: Duration = Duration::from_millis(500);
/// The watcher loop's tick; also the debounce granularity.
const TICK: Duration = Duration::from_millis(500);

/// Computes the directories/files to watch, refreshed on every resync.
/// Returning an empty list pauses the watcher (nothing to watch).
pub type WatchRootsProvider = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

/// Shared handle the app uses to swap watch roots (project opened/closed).
#[derive(Clone, Default)]
pub struct WatchRootsHandle {
    inner: Arc<Mutex<Vec<PathBuf>>>,
}

impl WatchRootsHandle {
    pub fn set(&self, roots: Vec<PathBuf>) {
        *self.inner.lock().unwrap() = roots;
    }
}

/// Whether one observed event should trigger a refresh. Events with no paths
/// or entirely inside `.git/` subtrees (CLI git operations churn them) are
/// noise.
fn should_emit(event: &Event) -> bool {
    if event.paths.is_empty() {
        return false;
    }
    event.paths.iter().any(|p| !is_in_git_dir(p))
}

fn is_in_git_dir(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new(".git"))
}

/// Pure debounce decision, split out for tests: `true` when the last emit is
/// old enough that this event should fire a new one.
fn debounce_allows(last_emit: Instant, now: Instant) -> bool {
    now.duration_since(last_emit) >= EMIT_DEBOUNCE
}

fn sync_watch_set(
    watcher: &mut notify::RecommendedWatcher,
    watched: &mut HashSet<PathBuf>,
    desired: Vec<PathBuf>,
) -> bool {
    let desired: HashSet<PathBuf> = desired.into_iter().collect();
    let mut changed = false;
    for stale in watched.difference(&desired).cloned().collect::<Vec<_>>() {
        let _ = watcher.unwatch(&stale);
        watched.remove(&stale);
        changed = true;
    }
    for path in desired {
        if watched.contains(&path) || !path.exists() {
            continue;
        }
        // A directory is watched recursively; a plain file (lock files)
        // non-recursively.
        let mode = if path.is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match watcher.watch(&path, mode) {
            Ok(()) => {
                watched.insert(path);
                changed = true;
            }
            Err(err) => {
                log::debug!("change_watcher: cannot watch {}: {err}", path.display());
            }
        }
    }
    changed
}

/// Start the watcher thread. Emits `skills-changed` (unit payload) on the
/// app handle whenever a relevant change lands outside the debounce window.
pub fn start_change_watcher<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    roots: WatchRootsHandle,
) {
    let provider: WatchRootsProvider = {
        let roots = roots.clone();
        Arc::new(move || roots.inner.lock().unwrap().clone())
    };
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::RecommendedWatcher::new(
            move |result| {
                let _ = tx.send(result);
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                log::error!("change_watcher: failed to create watcher: {err}");
                return;
            }
        };

        let mut watched = HashSet::new();
        let mut last_sync = Instant::now() - RESCAN_INTERVAL;
        let mut last_emit = Instant::now() - EMIT_DEBOUNCE;

        loop {
            if last_sync.elapsed() >= RESCAN_INTERVAL {
                // A watch-set change on its own is not a data change: syncing
                // only adds/removes watches, it never emits.
                sync_watch_set(&mut watcher, &mut watched, provider());
                last_sync = Instant::now();
            }

            match rx.recv_timeout(TICK) {
                Ok(Ok(event)) => {
                    let now = Instant::now();
                    if should_emit(&event) && debounce_allows(last_emit, now) {
                        if let Err(err) = app.emit(SKILLS_CHANGED_EVENT, ()) {
                            log::debug!("change_watcher: emit failed: {err}");
                        } else {
                            last_emit = now;
                        }
                    }
                }
                Ok(Err(err)) => {
                    log::debug!("change_watcher: watcher error: {err}");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_dir_events_are_noise() {
        let mk = |paths: &[&str]| notify::Event {
            kind: notify::EventKind::Modify(notify::event::ModifyKind::Any),
            paths: paths.iter().map(PathBuf::from).collect(),
            attrs: Default::default(),
        };
        assert!(!should_emit(&mk(&["/repo/.git/refs/heads"])));
        // Mixed events with at least one non-.git path are relevant.
        assert!(should_emit(&mk(&[
            "/repo/.git/FETCH_HEAD",
            "/repo/skills/a/SKILL.md"
        ])));
        assert!(should_emit(&mk(&["/home/u/.agents/skills/a/SKILL.md"])));
        // No paths at all: nothing to say.
        assert!(!should_emit(&mk(&[])));
    }

    #[test]
    fn debounce_window_is_half_open() {
        let now = Instant::now();
        assert!(!debounce_allows(now, now + EMIT_DEBOUNCE - Duration::from_millis(1)));
        assert!(debounce_allows(now, now + EMIT_DEBOUNCE));
    }

    #[test]
    fn sync_watch_set_adds_and_removes_watches() {
        let tmp = tempfile::tempdir().unwrap();
        let dir_a = tmp.path().join("a");
        let dir_b = tmp.path().join("b");
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();

        let (tx, _rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |r| {
            let _ = tx.send(r);
        })
        .unwrap();
        let mut watched = HashSet::new();

        assert!(sync_watch_set(&mut watcher, &mut watched, vec![dir_a.clone()]));
        assert!(watched.contains(&dir_a));
        // Idempotent re-sync.
        assert!(!sync_watch_set(&mut watcher, &mut watched, vec![dir_a.clone()]));
        // Swap to b: a is unwatched, b is watched.
        assert!(sync_watch_set(&mut watcher, &mut watched, vec![dir_b.clone()]));
        assert!(!watched.contains(&dir_a));
        assert!(watched.contains(&dir_b));
        // Missing paths are skipped silently; existing watches stay.
        let missing = tmp.path().join("not-there");
        assert!(!sync_watch_set(
            &mut watcher,
            &mut watched,
            vec![dir_b.clone(), missing.clone()]
        ));
        assert!(watched.contains(&dir_b));
        assert!(!watched.contains(&missing));
    }
}
