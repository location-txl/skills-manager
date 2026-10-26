//! Read-only parser for the Vercel `skills` CLI lock files.
//!
//! Global lock v3 (`$XDG_STATE_HOME/skills/.skill-lock.json` or
//! `~/.agents/.skill-lock.json`) and project lock v1
//! (`<project>/skills-lock.json`). These files are owned by the CLI; this
//! module never writes them. A lock that cannot be understood is reported as
//! such — it must never be mistaken for "no skills installed", and it must
//! never be rewritten by us.
//!
//! Schemas mirror vercel-labs/skills@1.7.4 `skill-lock.ts` / `local-lock.ts`.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

pub const GLOBAL_LOCK_VERSION: u32 = 3;
pub const PROJECT_LOCK_VERSION: u32 = 1;

/// Normalized view of one lock entry, shared by both lock kinds. Every field
/// is optional: the lock's schema is an external implementation detail and
/// unknown/renamed fields are ignored, never invented.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LockEntryView {
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub source_type: Option<String>,
    pub git_ref: Option<String>,
    pub skill_path: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Outcome of reading a lock file.
#[derive(Debug, PartialEq)]
pub enum LockRead {
    /// File does not exist — a legitimate "no tracked skills" state.
    Missing,
    /// File exists but its version is not the one this app understands
    /// (older *or* newer). Entries are unavailable; the CLI on disk decides
    /// what to do with it. We never rewrite it.
    Unsupported { version: Option<u32> },
    /// File exists but is not parseable JSON / wrong shape. Metadata
    /// unavailable, not an empty repository.
    Corrupt { reason: String },
    /// Parsed successfully. `skipped` lists entry keys that failed to
    /// deserialize individually (they are dropped from the listing, not fatal).
    Ok {
        version: u32,
        entries: BTreeMap<String, LockEntryView>,
        skipped: Vec<String>,
    },
}

impl LockRead {
    /// Tracked-skill entries, if the lock parsed. Empty for every other state.
    pub fn entries(&self) -> Option<&BTreeMap<String, LockEntryView>> {
        match self {
            LockRead::Ok { entries, .. } => Some(entries),
            _ => None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, LockRead::Ok { .. })
    }
}

/// Raw v3 global entry (skill-lock.ts `SkillLockEntry`).
#[derive(Debug, Deserialize)]
struct RawGlobalEntry {
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "sourceType")]
    source_type: Option<String>,
    #[serde(default, rename = "sourceUrl")]
    source_url: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    #[serde(default, rename = "skillPath")]
    skill_path: Option<String>,
    #[serde(default, rename = "installedAt")]
    installed_at: Option<String>,
    #[serde(default, rename = "updatedAt")]
    updated_at: Option<String>,
}

/// Raw v1 project entry (local-lock.ts `LocalSkillLockEntry`).
#[derive(Debug, Deserialize)]
struct RawProjectEntry {
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "sourceType")]
    source_type: Option<String>,
    #[serde(default, rename = "sourceUrl")]
    source_url: Option<String>,
    #[serde(default, rename = "ref")]
    git_ref: Option<String>,
    #[serde(default, rename = "skillPath")]
    skill_path: Option<String>,
}

/// Read a lock file of either kind. `expected_version` selects the schema:
/// [`GLOBAL_LOCK_VERSION`] or [`PROJECT_LOCK_VERSION`].
pub fn read_lock_file(path: &Path, expected_version: u32) -> LockRead {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return LockRead::Missing,
        Err(e) => return LockRead::Corrupt { reason: e.to_string() },
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(e) => return LockRead::Corrupt { reason: e.to_string() },
    };
    let Some(obj) = value.as_object() else {
        return LockRead::Corrupt {
            reason: "lock file is not a JSON object".into(),
        };
    };
    // The CLI requires `version` to be a JSON number and `skills` an object;
    // anything else is treated by the CLI as an invalid lock, never as empty.
    let Some(version) = obj.get("version").and_then(|v| v.as_u64()) else {
        return LockRead::Corrupt {
            reason: "lock file has no numeric `version`".into(),
        };
    };
    let Some(skills) = obj.get("skills").and_then(|v| v.as_object()) else {
        return LockRead::Corrupt {
            reason: "lock file has no `skills` object".into(),
        };
    };
    let version = u32::try_from(version).unwrap_or(u32::MAX);
    if version != expected_version {
        return LockRead::Unsupported {
            version: Some(version),
        };
    }

    let mut entries = BTreeMap::new();
    let mut skipped = Vec::new();
    for (name, raw) in skills {
        match parse_entry(raw, expected_version) {
            Some(view) => {
                entries.insert(name.clone(), view);
            }
            None => skipped.push(name.clone()),
        }
    }
    LockRead::Ok {
        version,
        entries,
        skipped,
    }
}

fn parse_entry(raw: &serde_json::Value, version: u32) -> Option<LockEntryView> {
    if !raw.is_object() {
        return None;
    }
    if version == GLOBAL_LOCK_VERSION {
        let entry: RawGlobalEntry = serde_json::from_value(raw.clone()).ok()?;
        Some(LockEntryView {
            source: entry.source,
            source_url: entry.source_url,
            source_type: entry.source_type,
            git_ref: entry.git_ref,
            skill_path: entry.skill_path,
            installed_at: entry.installed_at,
            updated_at: entry.updated_at,
        })
    } else {
        let entry: RawProjectEntry = serde_json::from_value(raw.clone()).ok()?;
        Some(LockEntryView {
            source: entry.source,
            source_url: entry.source_url,
            source_type: entry.source_type,
            git_ref: entry.git_ref,
            skill_path: entry.skill_path,
            installed_at: None,
            updated_at: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn write_lock(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn missing_file_is_missing_not_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let read = read_lock_file(&tmp.path().join(".skill-lock.json"), GLOBAL_LOCK_VERSION);
        assert_eq!(read, LockRead::Missing);
    }

    #[test]
    fn parses_global_v3_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{
              "version": 3,
              "skills": {
                "lark-base": {
                  "source": "open.feishu.cn",
                  "sourceType": "well-known",
                  "sourceUrl": "https://open.feishu.cn/.well-known/skills/lark-base/SKILL.md",
                  "skillFolderHash": "",
                  "installedAt": "2026-03-28T14:57:55.797Z",
                  "updatedAt": "2026-08-09T06:21:24.614Z"
                },
                "Some Skill": {
                  "source": "owner/repo",
                  "sourceType": "github",
                  "sourceUrl": "https://github.com/owner/repo",
                  "ref": "main",
                  "skillPath": "skills/some-skill/SKILL.md",
                  "skillFolderHash": "abc",
                  "installedAt": "2026-01-01T00:00:00.000Z",
                  "updatedAt": "2026-02-02T00:00:00.000Z",
                  "futureField": {"ignored": true}
                }
              },
              "dismissed": {"findSkillsPrompt": true},
              "lastSelectedAgents": ["claude-code"]
            }"#,
        );
        let read = read_lock_file(&path, GLOBAL_LOCK_VERSION);
        let LockRead::Ok { version, entries, skipped } = read else {
            panic!("expected Ok, got {read:?}");
        };
        assert_eq!(version, 3);
        assert!(skipped.is_empty());
        assert_eq!(entries.len(), 2);
        let lark = &entries["lark-base"];
        assert_eq!(lark.source.as_deref(), Some("open.feishu.cn"));
        assert_eq!(lark.source_type.as_deref(), Some("well-known"));
        assert!(lark.git_ref.is_none());
        let some = &entries["Some Skill"];
        assert_eq!(some.source.as_deref(), Some("owner/repo"));
        assert_eq!(some.git_ref.as_deref(), Some("main"));
        assert_eq!(
            some.skill_path.as_deref(),
            Some("skills/some-skill/SKILL.md")
        );
    }

    #[test]
    fn parses_project_v1_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            "skills-lock.json",
            r#"{
              "version": 1,
              "skills": {
                "pdf": {
                  "source": "vercel-labs/agent-skills",
                  "sourceType": "github",
                  "skillPath": "skills/pdf/SKILL.md",
                  "computedHash": "deadbeef"
                }
              }
            }"#,
        );
        let read = read_lock_file(&path, PROJECT_LOCK_VERSION);
        let LockRead::Ok { version, entries, skipped } = read else {
            panic!("expected Ok, got {read:?}");
        };
        assert_eq!(version, 1);
        assert!(skipped.is_empty());
        let pdf = &entries["pdf"];
        assert_eq!(pdf.source.as_deref(), Some("vercel-labs/agent-skills"));
        assert_eq!(pdf.source_type.as_deref(), Some("github"));
        // v1 carries no timestamps.
        assert!(pdf.installed_at.is_none());
        assert!(pdf.updated_at.is_none());
    }

    #[test]
    fn newer_version_is_unsupported_not_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{"version": 4, "skills": {"x": {"source": "a/b"}}}"#,
        );
        let read = read_lock_file(&path, GLOBAL_LOCK_VERSION);
        assert_eq!(
            read,
            LockRead::Unsupported { version: Some(4) }
        );
        assert!(read.entries().is_none());
    }

    #[test]
    fn older_version_is_unsupported_too() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{"version": 2, "skills": {}}"#,
        );
        assert_eq!(
            read_lock_file(&path, GLOBAL_LOCK_VERSION),
            LockRead::Unsupported { version: Some(2) }
        );
    }

    #[test]
    fn invalid_json_is_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(tmp.path(), ".skill-lock.json", "{ not json");
        assert!(matches!(
            read_lock_file(&path, GLOBAL_LOCK_VERSION),
            LockRead::Corrupt { .. }
        ));
    }

    #[test]
    fn non_numeric_version_is_corrupt() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{"version": "3", "skills": {}}"#,
        );
        assert!(matches!(
            read_lock_file(&path, GLOBAL_LOCK_VERSION),
            LockRead::Corrupt { .. }
        ));
    }

    #[test]
    fn garbage_entries_are_skipped_individually() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{
              "version": 3,
              "skills": {
                "good": {"source": "a/b", "sourceType": "github", "sourceUrl": "u"},
                "bad": "not an object",
                "worse": 42
              }
            }"#,
        );
        let read = read_lock_file(&path, GLOBAL_LOCK_VERSION);
        let LockRead::Ok { entries, skipped, .. } = read else {
            panic!("expected Ok");
        };
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key("good"));
        assert_eq!(skipped, vec!["bad".to_string(), "worse".to_string()]);
    }

    #[test]
    fn missing_optional_fields_are_tolerated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = write_lock(
            tmp.path(),
            ".skill-lock.json",
            r#"{"version": 3, "skills": {"minimal": {}}}"#,
        );
        let read = read_lock_file(&path, GLOBAL_LOCK_VERSION);
        let LockRead::Ok { entries, .. } = read else {
            panic!("expected Ok");
        };
        assert_eq!(
            entries["minimal"],
            LockEntryView {
                source: None,
                source_url: None,
                source_type: None,
                git_ref: None,
                skill_path: None,
                installed_at: None,
                updated_at: None,
            }
        );
    }

    #[test]
    fn unreadable_file_is_corrupt_with_reason() {
        // A directory at the lock path makes read_to_string fail with an IO
        // error other than NotFound.
        let tmp = tempfile::tempdir().unwrap();
        let dir_path = tmp.path().join(".skill-lock.json");
        std::fs::create_dir(&dir_path).unwrap();
        assert!(matches!(
            read_lock_file(&dir_path, GLOBAL_LOCK_VERSION),
            LockRead::Corrupt { .. }
        ));
    }
}
