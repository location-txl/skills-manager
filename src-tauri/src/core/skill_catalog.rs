//! Skill catalog: the single read-side aggregation of everything the GUI
//! knows about installed skills.
//!
//! Three inputs, in order of authority:
//! 1. **File system** — the shared canonical dir plus every agent-specific
//!    dir that exists. This alone decides whether a skill "exists".
//! 2. **Lock file** (v3 global / v1 project, read-only) — source and version
//!    metadata, `isTrackedByVercel`.
//! 3. **`skills ls --json`** — agent visibility as the installed CLI itself
//!    sees it (it filters to detected agents). Optional: without a CLI the
//!    catalog still works in degraded mode.
//!
//! Identity follows the CLI's rule: one skill per frontmatter `name` per
//! scope, no matter how many agent directories expose it (symlinks, copies,
//! or the shared dir). Broken symlinks are diagnosed, never deleted.

use super::agents::{self, AgentInfo, GlobalDir};
use super::lock_reader::{self, LockRead};
use super::scope::Scope;
use super::skill_metadata;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One skill as shown in the UI. Field names are the frontend DTO contract.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SkillView {
    /// `global:<path>` / `project:<path>` — UI identity only, never persisted.
    pub id: String,
    /// Real folder name on disk.
    pub slug: String,
    /// Display name from SKILL.md frontmatter (the CLI lock key).
    pub name: String,
    pub description: Option<String>,
    pub scope: String,
    pub path: String,
    pub status: String,
    pub source: Option<String>,
    pub source_url: Option<String>,
    pub source_type: Option<String>,
    pub installed_at: Option<String>,
    pub updated_at: Option<String>,
    pub agents: Vec<AgentExposureView>,
    pub is_tracked_by_vercel: bool,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct AgentExposureView {
    /// Agent key (from the controlled map) when known; otherwise the raw
    /// display name. Never passed back to the CLI unless it is a known key.
    pub id: String,
    pub name: String,
    /// shared | symlink | copy | native | unknown
    pub exposure: String,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResult {
    pub skills: Vec<SkillView>,
    /// Human-readable notes: stale lock entries, broken links, CLI errors.
    pub diagnostics: Vec<String>,
    /// ok | missing | unsupported | corrupt
    pub lock_state: String,
    /// Whether the CLI contributed to this snapshot.
    pub cli_available: bool,
}

/// Scan roots for a scope: the canonical shared dir plus each agent dir that
/// exists on disk. `agent == None` marks the canonical root.
fn scan_roots(scope: &Scope, home: &Path) -> Vec<(PathBuf, Option<&'static AgentInfo>)> {
    let canonical = match scope {
        Scope::Global => super::scope::global_skills_dir_for(home),
        Scope::Project(root) => super::scope::project_skills_dir_for(root),
    };
    let mut roots = vec![(canonical.clone(), None)];
    for agent in agents::AGENTS {
        let dir = match scope {
            Scope::Global => {
                if !agent.supports_global() {
                    continue;
                }
                match agent.global_skills_dir(home) {
                    // Agents whose global dir *is* the shared dir read it
                    // directly; that's exposure `shared`, not a second root.
                    Some(dir) if dir == canonical => continue,
                    Some(dir) => dir,
                    None => continue,
                }
            }
            Scope::Project(root) => {
                if agent.project_dir == ".agents/skills" {
                    continue;
                }
                agent.project_skills_dir(root)
            }
        };
        if dir.is_dir() && !roots.iter().any(|(d, _)| *d == dir) {
            roots.push((dir, Some(agent)));
        }
    }
    roots
}

/// One physical occurrence of a skill (in one scan root).
struct Occurrence {
    path: PathBuf,
    agent: Option<&'static AgentInfo>,
    exposure: String,
    /// Symlink present but its target is gone.
    broken: bool,
    name: String,
    description: Option<String>,
}

/// Read one scan root, collecting valid skill dirs and broken links.
fn scan_root(root: &Path, agent: Option<&'static AgentInfo>) -> Vec<Occurrence> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else { continue };
        let (exposure, broken) = if file_type.is_symlink() {
            match std::fs::metadata(&path) {
                // Follows the link; a dir target with SKILL.md is a skill.
                Ok(meta) if meta.is_dir() => ("symlink", false),
                _ => ("symlink", true),
            }
        } else if file_type.is_dir() {
            ("copy", false)
        } else {
            continue;
        };
        if broken {
            // Keep the name for diagnostics; nothing is parseable.
            let slug = entry.file_name().to_string_lossy().into_owned();
            found.push(Occurrence {
                path,
                agent,
                exposure: exposure.into(),
                broken: true,
                name: slug,
                description: None,
            });
            continue;
        }
        if std::fs::metadata(path.join("SKILL.md")).is_err() {
            continue;
        }
        let meta = skill_metadata::parse_skill_md(&path);
        let Some(name) = meta.name else {
            continue;
        };
        found.push(Occurrence {
            path,
            agent,
            exposure: exposure.into(),
            broken: false,
            name,
            description: meta.description,
        });
    }
    found
}

/// Public scan roots (canonical shared dir + existing agent dirs), used by
/// the document reader to validate paths against the same boundary.
pub fn scan_roots_public(scope: &Scope, home: &Path) -> Vec<(PathBuf, Option<&'static AgentInfo>)> {
    scan_roots(scope, home)
}

/// Build the catalog from disk + lock, without consulting the CLI.
/// `cli_available=false` marks degraded mode (read-only fallback).
pub fn build_catalog_from_disk(
    scope: &Scope,
    home: &Path,
    xdg_state_home: Option<&str>,
) -> CatalogResult {
    let mut diagnostics = Vec::new();
    let roots = scan_roots(scope, home);

    // Lock first: its state decides skill status globally.
    let lock_path = match scope {
        Scope::Global => super::scope::global_lock_path_for(home, xdg_state_home),
        Scope::Project(root) => super::scope::project_lock_path_for(root),
    };
    let lock_version = match scope {
        Scope::Global => lock_reader::GLOBAL_LOCK_VERSION,
        Scope::Project(_) => lock_reader::PROJECT_LOCK_VERSION,
    };
    let lock_read = lock_reader::read_lock_file(&lock_path, lock_version);
    let lock_state = match &lock_read {
        LockRead::Missing => "missing",
        LockRead::Unsupported { version } => {
            diagnostics.push(format!(
                "Lock file {} uses version {:?}; this app understands v{lock_version}. \
                 Metadata is unavailable; the skills themselves are unaffected.",
                lock_path.display(),
                version
            ));
            "unsupported"
        }
        LockRead::Corrupt { reason } => {
            diagnostics.push(format!(
                "Lock file {} could not be parsed ({reason}). Metadata is unavailable.",
                lock_path.display()
            ));
            "corrupt"
        }
        LockRead::Ok { .. } => "ok",
    }
    .to_string();

    // Name-keyed aggregation, mirroring the CLI's listInstalledSkills.
    struct Acc {
        canonical: Option<Occurrence>,
        occurrences: Vec<Occurrence>,
    }
    let mut by_name: BTreeMap<String, Acc> = BTreeMap::new();
    for (root, agent) in &roots {
        for occ in scan_root(root, *agent) {
            if occ.broken {
                // Broken links never create a skill; they only diagnose.
                let acc = by_name.entry(occ.name.clone()).or_insert(Acc {
                    canonical: None,
                    occurrences: Vec::new(),
                });
                acc.occurrences.push(occ);
                continue;
            }
            let is_canonical = agent.is_none();
            let acc = by_name.entry(occ.name.clone()).or_insert(Acc {
                canonical: None,
                occurrences: Vec::new(),
            });
            if is_canonical {
                if acc.canonical.is_none() {
                    acc.canonical = Some(occ);
                }
            } else {
                acc.occurrences.push(occ);
            }
        }
    }

    // Which agents see the shared dir directly (exposure `shared`).
    let shared_agents: Vec<&'static AgentInfo> = match scope {
        Scope::Global => agents::AGENTS
            .iter()
            .filter(|a| matches!(a.global_dir, GlobalDir::Shared))
            .collect(),
        Scope::Project(_) => agents::AGENTS
            .iter()
            .filter(|a| a.universal)
            .collect(),
    };

    let mut skills = Vec::new();
    for (name, acc) in &by_name {
        let Some(canonical) = &acc.canonical else {
            // Only broken links and/or agent-dir-only copies carry this name.
            let valid: Vec<&Occurrence> = acc.occurrences.iter().filter(|o| !o.broken).collect();
            if valid.is_empty() {
                for broken in &acc.occurrences {
                    diagnostics.push(format!(
                        "Broken link: {} ({} target missing) — left in place, not removed.",
                        broken.path.display(),
                        if broken.agent.is_some() { "agent dir" } else { "shared dir" }
                    ));
                }
                continue;
            }
            // Native skill(s): real dirs only inside agent directories. A
            // symlink mixed into the group (e.g. a share link) keeps its own
            // exposure — calling it "native" would make the GUI warn about
            // replacing a copy where only a link sits.
            let first = valid[0];
            skills.push(build_view(
                scope,
                name,
                first,
                valid
                    .iter()
                    .map(|o| AgentExposureView {
                        id: o.agent.unwrap().key.to_string(),
                        name: o.agent.unwrap().display_name.to_string(),
                        exposure: if o.exposure == "symlink" {
                            "symlink"
                        } else {
                            "native"
                        }
                        .into(),
                    })
                    .collect(),
                None,
                &lock_read,
                &lock_state,
            ));
            continue;
        };

        // Agents: every occurrence in an agent dir, plus shared-dir agents.
        let mut agent_map: BTreeMap<String, AgentExposureView> = BTreeMap::new();
        for occ in &acc.occurrences {
            if occ.broken {
                diagnostics.push(format!(
                    "Broken link: {} — left in place, not removed.",
                    occ.path.display()
                ));
                continue;
            }
            let agent = occ.agent.expect("non-canonical occurrence");
            // Copy = a real duplicate of the canonical dir; symlink = link.
            let exposure = if occ.exposure == "copy" { "copy" } else { "symlink" };
            agent_map.insert(
                agent.key.to_string(),
                AgentExposureView {
                    id: agent.key.to_string(),
                    name: agent.display_name.to_string(),
                    exposure: exposure.into(),
                },
            );
        }
        for agent in &shared_agents {
            agent_map.entry(agent.key.to_string()).or_insert_with(|| AgentExposureView {
                id: agent.key.to_string(),
                name: agent.display_name.to_string(),
                exposure: "shared".into(),
            });
        }
        skills.push(build_view(
            scope,
            name,
            canonical,
            agent_map.into_values().collect(),
            Some(canonical.path.to_path_buf()),
            &lock_read,
            &lock_state,
        ));
    }

    // Stale lock entries: tracked but nowhere on disk (T04). Diagnosis only.
    if let Some(entries) = lock_read.entries() {
        let sanitize = super::skills_cli::cli_sanitize_name;
        for key in entries.keys() {
            let present = by_name.keys().any(|n| n == key || sanitize(n) == sanitize(key));
            if !present {
                diagnostics.push(format!(
                    "Lock records skill \"{key}\" but no files were found on disk. \
                     The lock is owned by the skills CLI and was left untouched."
                ));
            }
        }
    }

    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    CatalogResult {
        skills,
        diagnostics,
        lock_state,
        cli_available: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_view(
    scope: &Scope,
    name: &str,
    representative: &Occurrence,
    agent_views: Vec<AgentExposureView>,
    canonical_path: Option<PathBuf>,
    lock_read: &LockRead,
    lock_state: &str,
) -> SkillView {
    let slug = canonical_path
        .as_ref()
        .or(Some(&representative.path))
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    let display_path = canonical_path.unwrap_or_else(|| representative.path.clone());
    let lock_entry = lock_read.entries().and_then(|entries| {
        entries
            .get(name)
            .or_else(|| {
                let sanitized = super::skills_cli::cli_sanitize_name(name);
                entries
                    .iter()
                    .find(|(k, _)| super::skills_cli::cli_sanitize_name(k) == sanitized)
                    .map(|(_, v)| v)
            })
            .cloned()
    });
    let status = if lock_state != "ok" && lock_state != "missing" {
        "metadata-unsupported"
    } else if lock_entry.is_some() {
        "installed"
    } else {
        "untracked"
    };
    SkillView {
        id: format!(
            "{}:{}",
            if scope.is_global() { "global" } else { "project" },
            display_path.display()
        ),
        slug,
        name: name.to_string(),
        description: representative.description.clone(),
        scope: if scope.is_global() { "global" } else { "project" }.into(),
        path: display_path.display().to_string(),
        status: status.into(),
        source: lock_entry.as_ref().and_then(|e| e.source.clone()),
        source_url: lock_entry.as_ref().and_then(|e| e.source_url.clone()),
        source_type: lock_entry.as_ref().and_then(|e| e.source_type.clone()),
        installed_at: lock_entry.as_ref().and_then(|e| e.installed_at.clone()),
        updated_at: lock_entry.as_ref().and_then(|e| e.updated_at.clone()),
        agents: agent_views,
        is_tracked_by_vercel: lock_entry.is_some(),
    }
}

/// One item of `skills ls --json` output (fields tolerant across versions:
/// 1.5.x emits only name/path/scope/agents).
#[derive(Debug, serde::Deserialize)]
pub struct CliSkillItem {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
}

/// Merge `ls --json` agent visibility into a disk-built catalog. The CLI's
/// view (it knows which agents are actually installed) replaces our
/// degraded-mode guesswork for skills it reports; lock metadata stays
/// authoritative for source fields.
pub fn merge_cli_listing(mut catalog: CatalogResult, items: &[CliSkillItem]) -> CatalogResult {
    catalog.cli_available = true;
    for item in items {
        let Some(skill) = catalog
            .skills
            .iter_mut()
            .find(|s| s.name == item.name || s.slug == item.name)
        else {
            // The CLI sees something our scan missed — report it rather than
            // trust either side blindly.
            catalog
                .diagnostics
                .push(format!("skills CLI listed \"{}\" which the disk scan did not find; skipped", item.name));
            continue;
        };
        let mut agent_map: BTreeMap<String, AgentExposureView> = BTreeMap::new();
        for display_name in &item.agents {
            let view = match agents::by_display_name(display_name) {
                Some(agent) => AgentExposureView {
                    id: agent.key.to_string(),
                    name: agent.display_name.to_string(),
                    // The CLI says the agent can see it; the how comes from
                    // our disk scan below.
                    exposure: "unknown".into(),
                },
                None => AgentExposureView {
                    id: display_name.clone(),
                    name: display_name.clone(),
                    exposure: "unknown".into(),
                },
            };
            agent_map.insert(view.id.clone(), view);
        }
        // Refine exposures from the disk scan we already did.
        for existing in &skill.agents {
            if let Some(view) = agent_map.get_mut(&existing.id) {
                view.exposure = existing.exposure.clone();
            } else {
                // Visible on disk but not reported by the CLI (agent was
                // uninstalled since, or the link is stale). Keep it with its
                // disk-derived exposure.
                agent_map.insert(existing.id.clone(), existing.clone());
            }
        }
        skill.agents = agent_map.into_values().collect();
        // `ls` source fields only fill gaps (1.5.x has none; the lock wins).
        if skill.source.is_none() {
            skill.source = item.source.clone();
        }
        if skill.source_url.is_none() {
            skill.source_url = item.source_url.clone();
        }
        if skill.source_type.is_none() {
            skill.source_type = item.source_type.clone();
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_skill(dir: &Path, name: &str, description: &str) -> PathBuf {
        let skill_dir = dir.join(super::super::skills_cli::cli_sanitize_name(name));
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n"),
        )
        .unwrap();
        skill_dir
    }

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn catalog_for(home: &Path) -> CatalogResult {
        build_catalog_from_disk(&Scope::Global, home, None)
    }

    #[test]
    fn catalog_serializes_frontend_lock_state_and_cli_availability() {
        let home = temp_home();
        let lock = home.path().join(".agents/.skill-lock.json");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();

        for (content, expected_state) in [
            (None, "missing"),
            (Some(r#"{"version":3,"skills":{}}"#), "ok"),
            (Some(r#"{"version":4,"skills":{}}"#), "unsupported"),
            (Some("{oops"), "corrupt"),
        ] {
            if let Some(content) = content {
                fs::write(&lock, content).unwrap();
            }
            let catalog = catalog_for(home.path());
            let json = serde_json::to_value(&catalog).unwrap();
            assert_eq!(json["lockState"], expected_state);
            assert_eq!(json["cliAvailable"], false);

            let merged = merge_cli_listing(catalog, &[]);
            let json = serde_json::to_value(&merged).unwrap();
            assert_eq!(json["lockState"], expected_state);
            assert_eq!(json["cliAvailable"], true);
        }
    }

    #[test]
    fn skill_serializes_frontend_source_metadata_and_tracking() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "tracked", "Tracked skill");
        write_skill(&shared, "untracked", "Local skill");
        fs::write(
            home.path().join(".agents/.skill-lock.json"),
            r#"{"version":3,"skills":{"tracked":{"source":"owner/repo","sourceType":"github","sourceUrl":"https://github.com/owner/repo","installedAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z"}}}"#,
        )
        .unwrap();

        let json = serde_json::to_value(catalog_for(home.path())).unwrap();
        let skills = json["skills"].as_array().unwrap();
        let tracked = skills.iter().find(|s| s["name"] == "tracked").unwrap();
        assert_eq!(tracked["isTrackedByVercel"], true);
        assert_eq!(tracked["sourceUrl"], "https://github.com/owner/repo");
        assert_eq!(tracked["sourceType"], "github");
        assert_eq!(tracked["installedAt"], "2026-01-01T00:00:00Z");
        assert_eq!(tracked["updatedAt"], "2026-01-02T00:00:00Z");

        let untracked = skills.iter().find(|s| s["name"] == "untracked").unwrap();
        assert_eq!(untracked["isTrackedByVercel"], false);
        for key in ["sourceUrl", "sourceType", "installedAt", "updatedAt"] {
            assert_eq!(untracked.get(key), Some(&serde_json::Value::Null));
        }
    }

    #[test]
    fn empty_home_is_empty_not_error() {
        let home = temp_home();
        let result = catalog_for(home.path());
        assert!(result.skills.is_empty());
        assert_eq!(result.lock_state, "missing");
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn skill_with_lock_is_installed_and_tracked() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "web-design", "Design guidance");
        let lock = home.path().join(".agents/.skill-lock.json");
        fs::write(
            &lock,
            r#"{"version":3,"skills":{"web-design":{"source":"vercel-labs/agent-skills","sourceType":"github","sourceUrl":"https://github.com/vercel-labs/agent-skills","installedAt":"2026-01-01T00:00:00.000Z","updatedAt":"2026-01-02T00:00:00.000Z"}}}"#,
        )
        .unwrap();

        let result = catalog_for(home.path());
        assert_eq!(result.lock_state, "ok");
        assert_eq!(result.skills.len(), 1);
        let s = &result.skills[0];
        assert_eq!(s.name, "web-design");
        assert_eq!(s.slug, "web-design");
        assert_eq!(s.status, "installed");
        assert!(s.is_tracked_by_vercel);
        assert_eq!(s.source.as_deref(), Some("vercel-labs/agent-skills"));
        assert_eq!(s.source_type.as_deref(), Some("github"));
        assert!(s.path.ends_with(".agents/skills/web-design"));
    }

    #[test]
    fn skill_without_lock_is_untracked() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "local-thing", "no lock entry");
        let result = catalog_for(home.path());
        assert_eq!(result.skills[0].status, "untracked");
        assert!(!result.skills[0].is_tracked_by_vercel);
        assert!(result.skills[0].source.is_none());
    }

    #[test]
    fn lock_record_without_files_is_diagnostic_only() {
        let home = temp_home();
        let lock = home.path().join(".agents/.skill-lock.json");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        fs::write(
            &lock,
            r#"{"version":3,"skills":{"ghost":{"source":"a/b","sourceType":"github","sourceUrl":"u"}}}"#,
        )
        .unwrap();
        let result = catalog_for(home.path());
        assert!(result.skills.is_empty(), "ghost must not count as installed");
        assert!(result
            .diagnostics
            .iter()
            .any(|d| d.contains("ghost") && d.contains("left untouched")));
    }

    #[test]
    fn unsupported_lock_version_marks_metadata_unsupported() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "some-skill", "x");
        let lock = home.path().join(".agents/.skill-lock.json");
        fs::write(&lock, r#"{"version":4,"skills":{}}"#).unwrap();
        let result = catalog_for(home.path());
        assert_eq!(result.lock_state, "unsupported");
        assert_eq!(result.skills[0].status, "metadata-unsupported");
        assert!(!result.skills[0].is_tracked_by_vercel);
    }

    #[test]
    fn corrupt_lock_marks_metadata_unsupported() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "some-skill", "x");
        fs::write(home.path().join(".agents/.skill-lock.json"), "{oops").unwrap();
        let result = catalog_for(home.path());
        assert_eq!(result.lock_state, "corrupt");
        assert_eq!(result.skills[0].status, "metadata-unsupported");
    }

    #[test]
    fn xdg_state_home_lock_location_is_used() {
        let home = temp_home();
        let xdg = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "xdg-skill", "x");
        fs::create_dir_all(xdg.path().join("skills")).unwrap();
        fs::write(
            xdg.path().join("skills/.skill-lock.json"),
            r#"{"version":3,"skills":{"xdg-skill":{"source":"a/b"}}}"#,
        )
        .unwrap();
        let result = build_catalog_from_disk(
            &Scope::Global,
            home.path(),
            Some(xdg.path().to_str().unwrap()),
        );
        assert_eq!(result.lock_state, "ok");
        assert!(result.skills[0].is_tracked_by_vercel);
    }

    #[cfg(unix)]
    #[test]
    fn agent_symlink_dedups_into_one_skill_with_exposure() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        let target = write_skill(&shared, "shared-skill", "d");
        let claude = home.path().join(".claude/skills");
        fs::create_dir_all(&claude).unwrap();
        std::os::unix::fs::symlink(&target, claude.join("shared-skill")).unwrap();

        let result = catalog_for(home.path());
        assert_eq!(result.skills.len(), 1, "one skill, not two");
        let s = &result.skills[0];
        let claude_view = s.agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert_eq!(claude_view.exposure, "symlink");
        // Shared-dir agents are listed too.
        assert!(s.agents.iter().any(|a| a.exposure == "shared"));
        assert!(s.path.ends_with(".agents/skills/shared-skill"));
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_diagnosed_not_listed() {
        let home = temp_home();
        let claude = home.path().join(".claude/skills");
        fs::create_dir_all(&claude).unwrap();
        std::os::unix::fs::symlink("/nowhere/at/all", claude.join("gone-skill")).unwrap();
        let result = catalog_for(home.path());
        assert!(result.skills.is_empty());
        assert!(result.diagnostics.iter().any(|d| d.contains("gone-skill")));
    }

    #[test]
    fn copy_in_agent_dir_and_canonical_dedups_as_copy() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "copied", "d");
        let cursor = home.path().join(".cursor/skills");
        write_skill(&cursor, "copied", "d");
        let result = catalog_for(home.path());
        assert_eq!(result.skills.len(), 1);
        let view = result.skills[0]
            .agents
            .iter()
            .find(|a| a.id == "cursor")
            .unwrap();
        assert_eq!(view.exposure, "copy");
    }

    #[test]
    fn agent_only_skill_is_native() {
        let home = temp_home();
        let claude = home.path().join(".claude/skills");
        write_skill(&claude, "claude-only", "d");
        let result = catalog_for(home.path());
        assert_eq!(result.skills.len(), 1);
        let s = &result.skills[0];
        assert_eq!(s.status, "untracked");
        let view = s.agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert_eq!(view.exposure, "native");
        assert!(s.path.ends_with(".claude/skills/claude-only"));
    }

    #[cfg(unix)]
    #[test]
    fn shared_native_skill_keeps_link_exposure() {
        // A native skill a share link was added to: the real dir stays
        // "native", the link must not be relabeled "native" too — the GUI
        // treats native/copy as "a replaceable copy" and would warn about
        // data loss where only a link sits.
        let home = temp_home();
        let claude = home.path().join(".claude/skills");
        write_skill(&claude, "claude-only", "d");
        let gemini = home.path().join(".gemini/skills");
        fs::create_dir_all(&gemini).unwrap();
        std::os::unix::fs::symlink(&claude.join("claude-only"), gemini.join("claude-only")).unwrap();

        let result = catalog_for(home.path());
        assert_eq!(result.skills.len(), 1);
        let s = &result.skills[0];
        let claude_view = s.agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert_eq!(claude_view.exposure, "native");
        let gemini_view = s.agents.iter().find(|a| a.id == "gemini-cli").unwrap();
        assert_eq!(gemini_view.exposure, "symlink");
    }

    #[test]
    fn project_scope_reads_project_dirs_and_v1_lock() {
        let home = temp_home();
        let project = temp_home();
        let shared = project.path().join(".agents/skills");
        write_skill(&shared, "proj-skill", "d");
        fs::write(
            project.path().join("skills-lock.json"),
            r#"{"version":1,"skills":{"proj-skill":{"source":"owner/repo","sourceType":"github","computedHash":"h"}}}"#,
        )
        .unwrap();
        let result = build_catalog_from_disk(
            &Scope::Project(project.path().to_path_buf()),
            home.path(),
            None,
        );
        assert_eq!(result.lock_state, "ok");
        assert_eq!(result.skills.len(), 1);
        let s = &result.skills[0];
        assert_eq!(s.scope, "project");
        assert_eq!(s.status, "installed");
        assert_eq!(s.source.as_deref(), Some("owner/repo"));
        assert!(s.installed_at.is_none(), "v1 lock has no timestamps");
    }

    #[test]
    fn dir_without_skill_md_is_ignored() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        fs::create_dir_all(shared.join("not-a-skill")).unwrap();
        fs::write(shared.join("not-a-skill/README.md"), "x").unwrap();
        let result = catalog_for(home.path());
        assert!(result.skills.is_empty());
    }

    #[test]
    fn missing_frontmatter_name_is_ignored_like_the_cli() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        let dir = shared.join("anonymous");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "no frontmatter here\n").unwrap();
        let result = catalog_for(home.path());
        assert!(result.skills.is_empty());
    }

    #[test]
    fn sanitized_lock_key_matches_display_name() {
        // Lock key "Some Skill" (frontmatter name) vs folder some-skill.
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "Some Skill", "d");
        fs::write(
            home.path().join(".agents/.skill-lock.json"),
            r#"{"version":3,"skills":{"Some Skill":{"source":"a/b"}}}"#,
        )
        .unwrap();
        let result = catalog_for(home.path());
        assert_eq!(result.skills[0].status, "installed");
        assert_eq!(result.skills[0].slug, "some-skill");
    }

    #[test]
    fn cli_listing_merges_agents_and_fills_source_gaps() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        write_skill(&shared, "merge-me", "d");
        let catalog = catalog_for(home.path());
        assert!(!catalog.cli_available, "disk-only until merged");

        let items = vec![CliSkillItem {
            name: "merge-me".into(),
            path: shared.join("merge-me").display().to_string(),
            scope: "global".into(),
            agents: vec!["Kilo Code".into(), "Mystery Agent".into()],
            source: Some("ls-source".into()),
            source_url: None,
            source_type: Some("github".into()),
        }];
        let merged = merge_cli_listing(catalog, &items);
        assert!(merged.cli_available);
        let s = &merged.skills[0];
        let kilo = s.agents.iter().find(|a| a.id == "kilo").unwrap();
        assert_eq!(kilo.name, "Kilo Code");
        // Unknown display name kept for display, flagged unknown.
        let mystery = s.agents.iter().find(|a| a.id == "Mystery Agent").unwrap();
        assert_eq!(mystery.exposure, "unknown");
        // `ls` source only fills gaps (no lock here).
        assert_eq!(s.source.as_deref(), Some("ls-source"));
    }

    #[test]
    fn cli_listing_refines_exposure_from_disk_scan() {
        let home = temp_home();
        let shared = home.path().join(".agents/skills");
        let target = write_skill(&shared, "refine", "d");
        let claude = home.path().join(".claude/skills");
        fs::create_dir_all(&claude).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, claude.join("refine")).unwrap();

        let catalog = catalog_for(home.path());
        let items = vec![CliSkillItem {
            name: "refine".into(),
            path: String::new(),
            scope: "global".into(),
            agents: vec!["Claude Code".into()],
            source: None,
            source_url: None,
            source_type: None,
        }];
        let merged = merge_cli_listing(catalog, &items);
        let s = &merged.skills[0];
        let claude_view = s.agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert_eq!(claude_view.exposure, "symlink");
    }
}
