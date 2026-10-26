//! Safe SKILL.md reading for the detail view.
//!
//! The path must resolve inside one of the scope's scan roots (the shared
//! dir or an agent dir we would have listed from); symlinks the CLI created
//! are followed only one hop, to their own skill dir. Content is size-capped
//! so a huge file can't balloon the webview. Read-only, of course.

use super::scope::Scope;
use std::path::Path;

/// Hard cap on the SKILL.md payload handed to the webview.
pub const MAX_SKILL_MD_BYTES: u64 = 512 * 1024;

#[derive(Debug, PartialEq)]
pub enum ReadDocError {
    /// Path escapes every scan root (or points at random files).
    OutsideScope,
    NotFound,
    TooLarge,
    Io(String),
}

#[derive(Debug)]
pub struct SkillDocument {
    pub markdown: String,
    pub truncated: bool,
}

/// Read the SKILL.md under `skill_dir` for the given scope.
pub fn read_skill_document(scope: &Scope, skill_dir: &Path, home: &Path) -> Result<SkillDocument, ReadDocError> {
    let roots = super::skill_catalog::scan_roots_public(scope, home);
    // The requested dir must sit inside a scan root (no canonicalize: for a
    // symlinked agent-dir entry the raw path is under the root, while its
    // target may live anywhere — that's the CLI's own layout).
    let inside = roots.iter().any(|(root, _)| {
        skill_dir.starts_with(root)
    });
    if !inside {
        return Err(ReadDocError::OutsideScope);
    }
    let file = skill_dir.join("SKILL.md");
    let meta = std::fs::metadata(&file).map_err(|_| ReadDocError::NotFound)?;
    if !meta.is_file() {
        return Err(ReadDocError::NotFound);
    }
    if meta.len() > MAX_SKILL_MD_BYTES {
        return Err(ReadDocError::TooLarge);
    }
    let bytes = std::fs::read(&file).map_err(|e| ReadDocError::Io(e.to_string()))?;
    let (content, truncated) = if bytes.len() as u64 > MAX_SKILL_MD_BYTES {
        (&bytes[..MAX_SKILL_MD_BYTES as usize], true)
    } else {
        (&bytes[..], false)
    };
    Ok(SkillDocument {
        markdown: String::from_utf8_lossy(content).into_owned(),
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_skill(dir: &Path, name: &str) -> std::path::PathBuf {
        let skill_dir = dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n\nbody\n"),
        )
        .unwrap();
        skill_dir
    }

    #[test]
    fn reads_a_skill_inside_the_shared_root() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".agents/skills");
        let skill = write_skill(&shared, "alpha");
        let doc = read_skill_document(&Scope::Global, &skill, home.path()).unwrap();
        assert!(doc.markdown.contains("name: alpha"));
        assert!(!doc.truncated);
    }

    #[test]
    fn reads_a_skill_inside_an_agent_root() {
        let home = tempfile::tempdir().unwrap();
        let claude = home.path().join(".claude/skills");
        let skill = write_skill(&claude, "beta");
        let doc = read_skill_document(&Scope::Global, &skill, home.path()).unwrap();
        assert!(doc.markdown.contains("beta"));
    }

    #[test]
    fn rejects_paths_outside_every_root() {
        let home = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let skill = write_skill(elsewhere.path(), "secret");
        assert_eq!(
            read_skill_document(&Scope::Global, &skill, home.path()).unwrap_err(),
            ReadDocError::OutsideScope
        );
        // Traversal attempts too.
        let sneak = Path::new("/definitely/outside");
        assert_eq!(
            read_skill_document(&Scope::Global, sneak, home.path()).unwrap_err(),
            ReadDocError::OutsideScope
        );
    }

    #[test]
    fn missing_file_is_not_found() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".agents/skills");
        std::fs::create_dir_all(shared.join("empty")).unwrap();
        assert_eq!(
            read_skill_document(
                &Scope::Global,
                &shared.join("empty"),
                home.path()
            )
            .unwrap_err(),
            ReadDocError::NotFound
        );
    }

    #[test]
    fn oversized_file_is_rejected() {
        let home = tempfile::tempdir().unwrap();
        let shared = home.path().join(".agents/skills");
        let skill = write_skill(&shared, "huge");
        let big = vec![b'a'; (MAX_SKILL_MD_BYTES + 1) as usize];
        std::fs::write(skill.join("SKILL.md"), &big).unwrap();
        assert_eq!(
            read_skill_document(&Scope::Global, &skill, home.path()).unwrap_err(),
            ReadDocError::TooLarge
        );
    }
}
