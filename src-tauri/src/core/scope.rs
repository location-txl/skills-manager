//! Scope and path boundaries for the Vercel `skills` CLI world.
//!
//! The CLI owns every path computed here; this module only mirrors its rules
//! (skill-lock.ts / installer.ts at skills@1.7.4) so the GUI reads the same
//! locations the CLI writes. Nothing in this module writes to disk.

use std::path::{Path, PathBuf};

/// Canonical shared skills dir for a scope:
/// global `~/.agents/skills`, project `<root>/.agents/skills`.
pub fn canonical_skills_dir(global: bool, root: Option<&Path>) -> PathBuf {
    match (global, root) {
        (true, _) => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".agents").join("skills")
        }
        (false, Some(root)) => root.join(".agents").join("skills"),
        (false, None) => PathBuf::from(".agents").join("skills"),
    }
}

/// Global shared skills dir for an explicit home (injectable for tests).
pub fn global_skills_dir_for(home: &Path) -> PathBuf {
    home.join(".agents").join("skills")
}

/// Project skills dir for an explicit root.
pub fn project_skills_dir_for(root: &Path) -> PathBuf {
    root.join(".agents").join("skills")
}

/// Global lock file path, mirroring skill-lock.ts `getSkillLockPath()`:
/// `$XDG_STATE_HOME/skills/.skill-lock.json` when that variable is set to a
/// non-empty value, otherwise `~/.agents/.skill-lock.json`.
/// An empty variable is falsy in the CLI's JS, so it falls back too.
pub fn global_lock_path_for(home: &Path, xdg_state_home: Option<&str>) -> PathBuf {
    match xdg_state_home.filter(|s| !s.is_empty()) {
        Some(xdg) => Path::new(xdg).join("skills").join(".skill-lock.json"),
        None => home.join(".agents").join(".skill-lock.json"),
    }
}

/// Project lock file path (`<root>/skills-lock.json`), mirroring local-lock.ts.
pub fn project_lock_path_for(root: &Path) -> PathBuf {
    root.join("skills-lock.json")
}

/// The scope a skill listing operates on. `Project` roots are absolute,
/// canonicalized paths validated by [`validate_project_root`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Global,
    Project(PathBuf),
}

impl Scope {
    pub fn is_global(&self) -> bool {
        matches!(self, Scope::Global)
    }
}

/// Validation outcome for a candidate project root.
#[derive(Debug, PartialEq)]
pub enum ProjectRootCheck {
    Ok,
    NotAbsolute,
    DoesNotExist,
    NotADirectory,
    /// The path is inside the old app's private repo; we never read or write
    /// there (refactor rule: legacy data is left alone on disk).
    InsideLegacySkillsManager,
}

/// Validate a user-selected project root. Read-only; does not canonicalize
/// or create anything.
pub fn validate_project_root(root: &Path) -> ProjectRootCheck {
    if !root.is_absolute() {
        return ProjectRootCheck::NotAbsolute;
    }
    let meta = match std::fs::metadata(root) {
        Ok(meta) => meta,
        Err(_) => return ProjectRootCheck::DoesNotExist,
    };
    if !meta.is_dir() {
        return ProjectRootCheck::NotADirectory;
    }
    if is_inside_legacy_repo(root) {
        return ProjectRootCheck::InsideLegacySkillsManager;
    }
    ProjectRootCheck::Ok
}

/// True when the path is the old app's private central repo (`~/.skills-manager`)
/// or somewhere inside it. The new app must ignore this tree entirely.
pub fn is_inside_legacy_repo(path: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let legacy = home.join(".skills-manager");
    path.starts_with(&legacy)
}

/// Normalize a project root for use as a `Scope::Project`: absolute +
/// canonicalized (symlinks resolved) so watch/scan paths line up.
pub fn normalize_project_root(root: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_lock_prefers_xdg_state_home() {
        let home = Path::new("/home/tester");
        let p = global_lock_path_for(home, Some("/xdg/state"));
        assert_eq!(p, Path::new("/xdg/state/skills/.skill-lock.json"));
    }

    #[test]
    fn empty_xdg_state_home_falls_back_to_home() {
        let home = Path::new("/home/tester");
        let p = global_lock_path_for(home, Some(""));
        assert_eq!(p, home.join(".agents/.skill-lock.json"));
    }

    #[test]
    fn global_lock_defaults_to_agents_dir() {
        let home = Path::new("/home/tester");
        let p = global_lock_path_for(home, None);
        assert_eq!(p, home.join(".agents/.skill-lock.json"));
    }

    #[test]
    fn canonical_dirs_follow_the_cli_layout() {
        let home = Path::new("/home/tester");
        assert_eq!(
            global_skills_dir_for(home),
            home.join(".agents/skills")
        );
        let root = Path::new("/proj");
        assert_eq!(
            project_skills_dir_for(root),
            root.join(".agents/skills")
        );
        assert_eq!(project_lock_path_for(root), root.join("skills-lock.json"));
    }

    #[test]
    fn rejects_relative_and_missing_roots() {
        assert_eq!(
            validate_project_root(Path::new("relative/thing")),
            ProjectRootCheck::NotAbsolute
        );
        let missing = std::env::temp_dir().join("definitely-not-here-9817");
        assert_eq!(
            validate_project_root(&missing),
            ProjectRootCheck::DoesNotExist
        );
    }

    #[test]
    fn rejects_file_paths_as_roots() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(
            validate_project_root(tmp.path()),
            ProjectRootCheck::NotADirectory
        );
    }

    #[test]
    fn accepts_real_directories() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(validate_project_root(tmp.path()), ProjectRootCheck::Ok);
    }

    #[test]
    fn detects_legacy_repo_paths() {
        if let Some(home) = dirs::home_dir() {
            let legacy = home.join(".skills-manager").join("skills");
            assert!(is_inside_legacy_repo(&legacy));
            let outside = home.join("Projects");
            assert!(!is_inside_legacy_repo(&outside));
        }
    }
}
