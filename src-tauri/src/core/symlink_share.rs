//! Self-managed symlink sharing — link an agent's skills dir at an existing
//! skill folder, without going through the Vercel `skills` CLI.
//!
//! Why not `skills add`: re-adding an installed skill deploys through the
//! canonical dir, deletes whatever real folder sits at an agent dir before
//! linking, rebuilds canonical when the upstream hash moved, and forces a
//! plain copy when only one target dir is involved. Sharing a skill that is
//! already on disk only needs a directory link pointing at the folder it
//! already lives in, so the GUI does that itself.
//!
//! The target classification, replacement policy and type-specific removal
//! are ported from the legacy sync_engine (deleted in phase D), minus its
//! store-backed `Recorded` policy: classify from `symlink_metadata` only,
//! refuse to destroy anything not provably ours without an explicit user
//! confirmation, and remove exactly the object that was classified so a real
//! folder appearing after the check errors instead of silently vanishing.
//!
//! These links are invisible to the CLI's lock file: `skills update` never
//! touches them, while `skills remove` (which scans agent dirs) and the
//! GUI catalog still see them as symlink exposures.

use std::io;
use std::path::{Path, PathBuf};

use crate::core::agents::{self, AgentInfo};
use crate::core::error::AppError;
use crate::core::scope::{self, Scope};

/// Per-agent outcome, in request order.
#[derive(Debug)]
pub struct ShareItem {
    pub agent: String,
    /// linked | replaced | replaced-copy | skipped | failed | conflict
    ///
    /// `conflict` = the target holds something of the user's (a real folder,
    /// a link to elsewhere, a file) that a confirmed re-run may replace;
    /// the GUI answers it with its retry confirmation.
    pub status: &'static str,
    pub detail: Option<String>,
}

/// Link every named agent's skills dir at `source`.
///
/// Errors abort the whole call (bad or unmanaged source, bad scope);
/// per-agent outcomes come back as items — `conflict` for a target the user
/// must confirm replacing, `failed`/`skipped` otherwise — so the rest of the
/// batch still applies.
pub fn share_skill(
    home: &Path,
    scope_kind: &Scope,
    source: &Path,
    agent_keys: &[String],
    replace_copies: bool,
) -> Result<Vec<ShareItem>, AppError> {
    if !source.is_absolute() {
        return Err(AppError::invalid_input("source path must be absolute"));
    }
    let source = source
        .canonicalize()
        .map_err(|e| AppError::io(format!("source folder not accessible: {e}")))?;
    if !source.is_dir() {
        return Err(AppError::invalid_input("source is not a folder"));
    }
    if !source.join("SKILL.md").is_file() {
        return Err(AppError::invalid_input("source folder has no SKILL.md"));
    }
    if !is_managed_source(&source, home, scope_kind) {
        // The GUI only ever shares folders its catalog listed; anything else
        // reaching this command (a direct IPC invoke) gets no destructive
        // authorization from it.
        return Err(AppError::invalid_input(
            "source folder is not inside a managed skills directory",
        ));
    }
    let name = source
        .file_name()
        .ok_or_else(|| AppError::invalid_input("source folder has no name"))?
        .to_owned();
    let name = Path::new(&name);

    let mut items = Vec::with_capacity(agent_keys.len());
    for key in agent_keys {
        match agents::by_key(key) {
            Some(agent) => {
                items.push(share_one(home, scope_kind, agent, &source, name, replace_copies))
            }
            None => items.push(ShareItem {
                agent: key.clone(),
                status: "failed",
                detail: Some("unknown agent".to_string()),
            }),
        }
    }
    Ok(items)
}

fn share_one(
    home: &Path,
    scope: &Scope,
    agent: &AgentInfo,
    source: &Path,
    name: &Path,
    replace_copies: bool,
) -> ShareItem {
    let item = |status: &'static str, detail: Option<String>| ShareItem {
        agent: agent.display_name.to_string(),
        status,
        detail,
    };
    let failed = |detail: String| item("failed", Some(detail));

    let dir = match scope {
        Scope::Global => agent.global_skills_dir(home),
        Scope::Project(root) => Some(agent.project_skills_dir(root)),
    };
    let Some(dir) = dir else {
        return item("skipped", Some("no global skills dir".to_string()));
    };

    // A link inside the shared dir exposes the skill to every agent reading
    // it directly, which is not what a per-agent share means. Path equality
    // covers both scopes (global `~/.agents/skills`, project `<root>/.agents/skills`).
    let shared = match scope {
        Scope::Global => scope::global_skills_dir_for(home),
        Scope::Project(root) => scope::project_skills_dir_for(root),
    };
    if dir == shared {
        return item(
            "skipped",
            Some("reads the shared dir directly; a link there would expose the skill to every universal agent".to_string()),
        );
    }

    let target = dir.join(name);

    // The source itself may live in this agent's dir — a confirmed replace
    // must never delete the folder being shared. (Canonicalize follows links,
    // so a link at the target falls through to the normal classification.)
    if std::fs::canonicalize(&target).is_ok_and(|p| p == source) {
        return item(
            "skipped",
            Some("this folder already lives in this agent's dir".to_string()),
        );
    }

    let state = match classify_target(&target, Some(source)) {
        Ok(state) => state,
        // A target we cannot even stat is not evidence that it is ours.
        Err(e) => return failed(format!("cannot inspect target: {e}")),
    };
    if matches!(state, TargetState::LinkToSource) {
        return item("skipped", Some("already linked here".to_string()));
    }
    let policy = if replace_copies {
        ReplacePolicy::UserConfirmed
    } else {
        ReplacePolicy::NoClobber
    };
    if !policy.permits(state) {
        let what = match state {
            TargetState::ForeignLink => "a link to another folder",
            TargetState::RealDir => "a real folder",
            TargetState::RealFile => "a file",
            _ => unreachable!("Absent and LinkToSource are always permitted"),
        };
        // Not an error: the GUI answers this with its retry confirmation
        // (`conflict` is the only route to a confirmed re-run).
        return item(
            "conflict",
            Some(format!("{what} already exists here; confirm replacing it")),
        );
    }

    if let Err(e) = std::fs::create_dir_all(&dir) {
        return failed(format!("could not create {}: {e}", dir.display()));
    }
    // Remove exactly what was classified, so a real folder that appeared
    // between the check and here fails the removal instead of being eaten.
    if let Err(e) = remove_classified_target(&target, state) {
        return failed(format!("could not clear the existing target: {e}"));
    }
    let status = match state {
        TargetState::ForeignLink => "replaced",
        TargetState::RealDir => "replaced-copy",
        _ => "linked",
    };

    // The link is computed against the dir's physical path: where the home
    // itself sits behind a symlink (macOS temp dirs, network homes), counting
    // ".." lexically against the canonicalized source would walk past root.
    let link_base = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
    let rel = relative_target(&link_base, source);
    if let Err(e) = create_dir_link(source, &rel, &target) {
        return failed(format!("could not create link: {e}"));
    }
    if !symlink_points_to(&target, source) {
        let _ = std::fs::remove_file(&target);
        return failed("created link did not point back at the source".to_string());
    }
    // Some agents physically read the same dir (zencoder/zenflow share
    // ~/.zencoder/skills; amp/replit/universal share ~/.config/agents/skills):
    // the link is visible to them too, so say so instead of leaking silently.
    let co_owners = co_owner_names(&dir, home, scope, agent);
    let detail = if co_owners.is_empty() {
        target.display().to_string()
    } else {
        format!(
            "{} — also visible to {}",
            target.display(),
            co_owners.join(", ")
        )
    };
    item(status, Some(detail))
}

/// Display names of other agents whose skills dir is physically this same
/// dir (the AGENTS table has a few colliding entries that are not the
/// canonical shared dir).
fn co_owner_names(
    dir: &Path,
    home: &Path,
    scope: &Scope,
    self_agent: &AgentInfo,
) -> Vec<String> {
    agents::AGENTS
        .iter()
        .filter(|a| a.key != self_agent.key)
        .filter(|a| {
            let d = match scope {
                Scope::Global => a.global_skills_dir(home),
                Scope::Project(root) => Some(a.project_skills_dir(root)),
            };
            d.as_ref().is_some_and(|d| d == dir)
        })
        .map(|a| a.display_name.to_string())
        .collect()
}

/// A skill folder the GUI could have listed: one sitting directly inside a
/// managed skills dir — the scope's own roots, plus the global roots (a
/// global skill may legitimately be shared into a project). Anything else,
/// however valid a folder, is not a source this app vouches for.
fn is_managed_source(source: &Path, home: &Path, scope: &Scope) -> bool {
    let Some(parent) = source.parent() else {
        return false;
    };
    let mut roots = vec![scope::global_skills_dir_for(home)];
    for agent in agents::AGENTS {
        if let Some(dir) = agent.global_skills_dir(home) {
            roots.push(dir);
        }
    }
    if let Scope::Project(root) = scope {
        roots.push(scope::project_skills_dir_for(root));
        for agent in agents::AGENTS {
            roots.push(agent.project_skills_dir(root));
        }
    }
    // `parent` comes from the canonicalized source; the roots are lexical
    // home-joined paths, so compare against their physical form (a home
    // behind a symlink, like macOS temp dirs, would otherwise mismatch).
    roots.iter().any(|r| r.canonicalize().is_ok_and(|c| &c == parent))
}

// ---------------------------------------------------------------------------
// Ported from the legacy sync_engine (pre-phase-D).
// ---------------------------------------------------------------------------

/// What is actually sitting at a deployment target right now.
///
/// Classified from `symlink_metadata` only — never `is_dir()`, which follows
/// links and would report a Windows junction or a directory symlink as a real
/// directory, sending it down the `remove_dir_all` path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetState {
    /// Nothing there. Deploying cannot destroy anything.
    Absent,
    /// A link that resolves to the source we are about to deploy — ours by
    /// construction, and unlinking it cannot touch the source.
    LinkToSource,
    /// A link pointing somewhere else, or a dangling one. The link itself is
    /// user configuration even when its pointee is gone.
    ForeignLink,
    /// A real directory.
    RealDir,
    /// A real file. We never deploy a bare file, so this is always the user's.
    RealFile,
}

/// Whether the caller may destroy what the classification found. The legacy
/// engine had a third, store-backed variant (`Recorded { mode }`); with no
/// store the choice collapses to "prove it is ours" vs "the user confirmed".
#[derive(Debug, Clone, Copy)]
enum ReplacePolicy {
    /// Only proceed when nothing of the user's can be lost.
    NoClobber,
    /// Explicit user intent, gathered by a confirmation in the GUI.
    UserConfirmed,
}

impl ReplacePolicy {
    fn permits(self, state: TargetState) -> bool {
        match state {
            // Nothing of the user's can be lost either way.
            TargetState::Absent | TargetState::LinkToSource => true,
            // Everything else — a foreign (or dangling) link, a real folder,
            // a bare file — is the user's until they explicitly confirm.
            _ => matches!(self, ReplacePolicy::UserConfirmed),
        }
    }
}

/// Classify what is at `target` without following links.
///
/// `source` is the deployment source, used only to recognize a link that
/// already points at it. Pass `None` when there is no source to compare
/// against (an empty path would canonicalize to the CWD and turn a user's
/// link into "ours").
fn classify_target(target: &Path, source: Option<&Path>) -> io::Result<TargetState> {
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(TargetState::Absent),
        Err(err) => return Err(err),
    };
    if metadata.file_type().is_symlink() {
        if source.is_some_and(|source| symlink_points_to(target, source)) {
            return Ok(TargetState::LinkToSource);
        }
        return Ok(TargetState::ForeignLink);
    }
    if metadata.is_dir() {
        Ok(TargetState::RealDir)
    } else {
        Ok(TargetState::RealFile)
    }
}

fn symlink_points_to(target: &Path, source: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(target) else {
        return false;
    };
    if !metadata.file_type().is_symlink() {
        return false;
    }
    let Ok(link_target) = std::fs::read_link(target) else {
        return false;
    };
    let resolved = if link_target.is_absolute() {
        link_target
    } else {
        target
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(link_target)
    };
    if resolved == source {
        return true;
    }
    match (resolved.canonicalize(), source.canonicalize()) {
        (Ok(link), Ok(src)) => link == src,
        _ => false,
    }
}

/// Remove a target that was already classified, using an operation that only
/// works on that kind of object.
///
/// This is the difference between "we checked, then deleted whatever we
/// found" and "we deleted the thing we checked". If a real directory replaces
/// a link between the classification and this call, `remove_file` fails with
/// an error instead of `remove_dir_all` quietly eating it — the exact race
/// that makes a check-then-remove sequence still unsafe. `RealDir` is only
/// reachable under [`ReplacePolicy::UserConfirmed`].
fn remove_classified_target(target: &Path, state: TargetState) -> io::Result<()> {
    match state {
        TargetState::Absent => Ok(()),
        TargetState::LinkToSource | TargetState::ForeignLink | TargetState::RealFile => {
            std::fs::remove_file(target)
        }
        TargetState::RealDir => std::fs::remove_dir_all(target),
    }
}

// ---------------------------------------------------------------------------
// New in this module: relative links in the CLI's agent-dir layout.
// ---------------------------------------------------------------------------

/// Relative path from `from_dir` to `to`, so links survive a moved home the
/// same way the CLI's own relative links do. Both must be absolute, and
/// `from_dir` should be the dir's physical path (see `share_one`).
fn relative_target(from_dir: &Path, to: &Path) -> PathBuf {
    let from: Vec<_> = from_dir.components().collect();
    let to: Vec<_> = to.components().collect();
    let mut shared = 0;
    while shared < from.len() && shared < to.len() && from[shared] == to[shared] {
        shared += 1;
    }
    let mut rel = PathBuf::new();
    for _ in shared..from.len() {
        rel.push("..");
    }
    for comp in &to[shared..] {
        rel.push(comp.as_os_str());
    }
    rel
}

#[cfg(unix)]
fn create_dir_link(_source: &Path, rel: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(rel, link)
}

#[cfg(windows)]
fn create_dir_link(source: &Path, rel: &Path, link: &Path) -> io::Result<()> {
    match std::os::windows::fs::symlink_dir(rel, link) {
        Ok(()) => Ok(()),
        // Typical causes: missing SeCreateSymbolicLinkPrivilege, Developer
        // Mode disabled, or a non-NTFS volume. A directory junction needs no
        // privilege on local NTFS volumes and is equivalent for our purposes
        // (issue #126) — but it only accepts absolute targets. std reports a
        // junction back as a symlink (`is_symlink()`, `read_link`), so the
        // verification in `share_one` handles it like a real directory link.
        Err(err) => junction::create(source, link).or(Err(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// A real skill folder inside cursor's global dir.
    fn skill_in_cursor(home: &Path, name: &str) -> PathBuf {
        let dir = home.join(".cursor").join("skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: test\n---\nbody").unwrap();
        dir
    }

    fn share(
        home: &Path,
        source: &Path,
        keys: &[&str],
        replace_copies: bool,
    ) -> Vec<ShareItem> {
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        share_skill(home, &Scope::Global, source, &keys, replace_copies).unwrap()
    }

    #[test]
    fn creates_relative_link_into_existing_dir() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let items = share(home.path(), &source, &["gemini-cli"], false);

        assert_eq!(items[0].status, "linked", "detail: {:?}", items[0].detail);
        let link = home.path().join(".gemini").join("skills").join("web-design");
        assert_eq!(
            std::fs::read_link(&link).unwrap(),
            PathBuf::from("../../.cursor/skills/web-design")
        );
        assert!(std::fs::canonicalize(&link).unwrap() == source.canonicalize().unwrap());
    }

    #[test]
    fn creates_agent_dir_when_missing() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        // claude-code's dir does not exist yet; the share must create it.
        let items = share(home.path(), &source, &["claude-code"], false);
        assert_eq!(items[0].status, "linked", "detail: {:?}", items[0].detail);
        assert!(home.path().join(".claude").join("skills").join("web-design").is_dir());
    }

    #[test]
    fn same_link_is_skipped() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        share(home.path(), &source, &["gemini-cli"], false);
        let items = share(home.path(), &source, &["gemini-cli"], false);
        assert_eq!(items[0].status, "skipped");
    }

    #[test]
    fn foreign_link_returns_conflict_without_confirm() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let other = home.path().join(".gemini").join("skills").join("other-skill");
        std::fs::create_dir_all(&other).unwrap();
        let link = home.path().join(".gemini").join("skills").join("web-design");
        std::os::unix::fs::symlink("other-skill", &link).unwrap();

        let items = share(home.path(), &source, &["gemini-cli"], false);
        assert_eq!(items[0].status, "conflict");
        // The user's link and what it points at are both untouched.
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(other.is_dir());
    }

    #[test]
    fn foreign_link_replaced_with_confirm() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let other = home.path().join(".gemini").join("skills").join("other-skill");
        std::fs::create_dir_all(&other).unwrap();
        let link = home.path().join(".gemini").join("skills").join("web-design");
        std::os::unix::fs::symlink("other-skill", &link).unwrap();

        let items = share(home.path(), &source, &["gemini-cli"], true);
        assert_eq!(items[0].status, "replaced");
        // The bystander the old link pointed at is untouched.
        assert!(other.is_dir());
        assert!(symlink_points_to(&link, &source.canonicalize().unwrap()));
    }

    #[test]
    fn real_folder_returns_conflict_without_flag() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let copy = home.path().join(".gemini").join("skills").join("web-design");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(copy.join("SKILL.md"), "local edits").unwrap();

        let items = share(home.path(), &source, &["gemini-cli"], false);
        assert_eq!(items[0].status, "conflict");
        assert_eq!(std::fs::read_to_string(copy.join("SKILL.md")).unwrap(), "local edits");
    }

    #[test]
    fn real_folder_replaced_with_flag() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let copy = home.path().join(".gemini").join("skills").join("web-design");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(copy.join("SKILL.md"), "local edits").unwrap();

        let items = share(home.path(), &source, &["gemini-cli"], true);
        assert_eq!(items[0].status, "replaced-copy");
        assert!(std::fs::symlink_metadata(&copy).unwrap().file_type().is_symlink());
    }

    #[test]
    fn real_file_returns_conflict() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let dir = home.path().join(".gemini").join("skills");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("web-design"), "not a folder").unwrap();

        let items = share(home.path(), &source, &["gemini-cli"], false);
        assert_eq!(items[0].status, "conflict");
        assert_eq!(
            std::fs::read_to_string(dir.join("web-design")).unwrap(),
            "not a folder"
        );
    }

    #[test]
    fn hosting_agent_is_skipped() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let items = share(home.path(), &source, &["cursor"], false);
        assert_eq!(items[0].status, "skipped");
    }

    #[test]
    fn hosting_agent_is_skipped_even_with_confirm() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let items = share(home.path(), &source, &["cursor"], true);
        // The guard fires before the policy: a confirmed replace must never
        // delete the very folder being shared.
        assert_eq!(items[0].status, "skipped");
        assert!(source.join("SKILL.md").is_file());
    }

    #[test]
    fn shared_dir_agent_is_skipped() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        // codex reads ~/.agents/skills directly — a link there would leak the
        // skill to every universal agent.
        let items = share(home.path(), &source, &["codex"], false);
        assert_eq!(items[0].status, "skipped");
        assert!(!home.path().join(".agents").join("skills").join("web-design").exists());
    }

    #[test]
    fn no_global_dir_agent_is_skipped() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let items = share(home.path(), &source, &["eve"], false);
        assert_eq!(items[0].status, "skipped");
    }

    #[test]
    fn unknown_agent_is_failed_item() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        let items = share(home.path(), &source, &["no-such-agent"], false);
        assert_eq!(items[0].status, "failed");
    }

    #[test]
    fn source_without_skill_md_is_rejected() {
        let home = temp_home();
        let plain = home.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let err = share_skill(
            home.path(),
            &Scope::Global,
            &plain,
            &["gemini-cli".to_string()],
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("SKILL.md"));
    }

    #[test]
    fn project_scope_links_into_project_dirs() {
        let home = temp_home();
        let project = home.path().join("proj");
        let source = skill_in_cursor(home.path(), "web-design");

        let keys = vec!["claude-code".to_string()];
        let items =
            share_skill(home.path(), &Scope::Project(project.clone()), &source, &keys, false)
                .unwrap();
        assert_eq!(items[0].status, "linked");
        assert!(project.join(".claude").join("skills").join("web-design").is_dir());

        // Universal agents read the project shared dir — skipped there too.
        let keys = vec!["codex".to_string()];
        let items =
            share_skill(home.path(), &Scope::Project(project.clone()), &source, &keys, false)
                .unwrap();
        assert_eq!(items[0].status, "skipped");
    }

    #[test]
    fn source_outside_managed_roots_is_rejected() {
        let home = temp_home();
        // A valid skill folder, but in a place no agent reads.
        let stray = home.path().join("somewhere-else").join("web-design");
        std::fs::create_dir_all(&stray).unwrap();
        std::fs::write(stray.join("SKILL.md"), "---\nname: test\n---\n").unwrap();

        let err = share_skill(
            home.path(),
            &Scope::Global,
            &stray,
            &["gemini-cli".to_string()],
            false,
        )
        .unwrap_err();
        assert!(err.to_string().contains("managed skills directory"));
    }

    #[test]
    fn co_owner_dir_is_annotated() {
        let home = temp_home();
        let source = skill_in_cursor(home.path(), "web-design");
        // zencoder and zenflow physically share ~/.zencoder/skills.
        let items = share(home.path(), &source, &["zencoder"], false);
        assert_eq!(items[0].status, "linked");
        assert!(
            items[0].detail.as_deref().unwrap().contains("Zenflow"),
            "detail: {:?}",
            items[0].detail
        );
        // The co-owner then resolves to the same (now linked) target.
        let items = share(home.path(), &source, &["zenflow"], false);
        assert_eq!(items[0].status, "skipped");
    }
}
