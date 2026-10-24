//! Controlled agent map — a snapshot of the Vercel `skills` CLI's agent
//! table at skills@1.7.4 (`src/agents.ts`, 79 entries).
//!
//! Why this exists: `skills ls --json` reports agents by **display name**,
//! while `-a`/`--agent` expects **agent keys**. Passing a display name where
//! a key is required is a silent no-op, so the GUI must translate through
//! this table and never echo a display name back to the CLI. Unknown display
//! names (from a newer CLI) are shown but cannot be targeted.
//!
//! It also mirrors each agent's skill directories so the GUI can scan disk
//! and watch the right places. Snapshot drift across CLI versions is a
//! known, accepted trade-off: the CLI's own `ls` output stays authoritative
//! for visibility, this table only enriches and translates.

use std::path::PathBuf;

/// Where an agent keeps its **global** skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalDir {
    /// Fixed path under the user's home (`~/.claude/skills` etc.).
    Home(&'static str),
    /// Under `$XDG_CONFIG_HOME` (or `~/.config` when unset), like the CLI's
    /// xdg-basedir usage.
    ConfigHome(&'static str),
    /// An agent-specific env override naming the agent's **base directory**,
    /// with a home-relative default: dir = `($VAR || ~/<base>)/<sub>`
    /// (e.g. `$CLAUDE_CONFIG_DIR/skills`, default `~/.claude/skills`).
    EnvOrHome { var: &'static str, base: &'static str, sub: &'static str },
    /// The shared canonical dir `~/.agents/skills`.
    Shared,
    /// First existing of `~/.openclaw/skills`, `~/.clawdbot/skills`,
    /// `~/.moltbot/skills`; default `~/.openclaw/skills`.
    OpenClaw,
    /// This agent has no global scope (eve, promptscript).
    None,
}

/// One agent in the controlled map.
#[derive(Debug, Clone)]
pub struct AgentInfo {
    /// CLI agent key — the only value ever passed to `-a`.
    pub key: &'static str,
    /// Display name exactly as `ls --json` reports it.
    pub display_name: &'static str,
    /// Project skills dir relative to the project root.
    pub project_dir: &'static str,
    pub global_dir: GlobalDir,
    /// `project_dir == ".agents/skills"`: the agent reads the shared dir
    /// directly instead of keeping its own copy.
    pub universal: bool,
}

impl AgentInfo {
    pub fn supports_global(&self) -> bool {
        !matches!(self.global_dir, GlobalDir::None)
    }

    /// Absolute global skills dir for this agent.
    pub fn global_skills_dir(&self, home: &std::path::Path) -> Option<PathBuf> {
        let xdg_config = || {
            std::env::var("XDG_CONFIG_HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        };
        match &self.global_dir {
            GlobalDir::Home(rel) => Some(home.join(rel)),
            GlobalDir::ConfigHome(rel) => Some(xdg_config().join(rel)),
            GlobalDir::EnvOrHome { var, base, sub } => {
                let base_dir = std::env::var(var)
                    .ok()
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(base));
                Some(base_dir.join(sub))
            }
            GlobalDir::Shared => Some(home.join(".agents").join("skills")),
            GlobalDir::OpenClaw => {
                for rel in [".openclaw", ".clawdbot", ".moltbot"] {
                    let dir = home.join(rel).join("skills");
                    if dir.exists() {
                        return Some(dir);
                    }
                }
                Some(home.join(".openclaw").join("skills"))
            }
            GlobalDir::None => None,
        }
    }

    /// Project skills dir under `root`.
    pub fn project_skills_dir(&self, root: &std::path::Path) -> PathBuf {
        root.join(self.project_dir)
    }
}

/// The controlled map, in the CLI's `Object.keys(agents)` order.
/// Source: vercel-labs/skills@1.7.4 src/agents.ts.
pub const AGENTS: &[AgentInfo] = &[
    AgentInfo { key: "aider-desk", display_name: "AiderDesk", project_dir: ".aider-desk/skills", global_dir: GlobalDir::Home(".aider-desk/skills"), universal: false },
    AgentInfo { key: "amp", display_name: "Amp", project_dir: ".agents/skills", global_dir: GlobalDir::ConfigHome("agents/skills"), universal: true },
    AgentInfo { key: "antigravity", display_name: "Antigravity", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".gemini/antigravity/skills"), universal: true },
    AgentInfo { key: "antigravity-cli", display_name: "Antigravity CLI", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".gemini/antigravity-cli/skills"), universal: true },
    AgentInfo { key: "astrbot", display_name: "AstrBot", project_dir: "data/skills", global_dir: GlobalDir::Home(".astrbot/data/skills"), universal: false },
    AgentInfo { key: "autohand-code", display_name: "Autohand Code CLI", project_dir: ".autohand/skills", global_dir: GlobalDir::EnvOrHome { var: "AUTOHAND_HOME", base: ".autohand", sub: "skills" }, universal: false },
    AgentInfo { key: "augment", display_name: "Augment", project_dir: ".augment/skills", global_dir: GlobalDir::Home(".augment/skills"), universal: false },
    AgentInfo { key: "bob", display_name: "IBM Bob", project_dir: ".bob/skills", global_dir: GlobalDir::Home(".bob/skills"), universal: false },
    AgentInfo { key: "claude-code", display_name: "Claude Code", project_dir: ".claude/skills", global_dir: GlobalDir::EnvOrHome { var: "CLAUDE_CONFIG_DIR", base: ".claude", sub: "skills" }, universal: false },
    AgentInfo { key: "openclaw", display_name: "OpenClaw", project_dir: "skills", global_dir: GlobalDir::OpenClaw, universal: false },
    AgentInfo { key: "cline", display_name: "Cline", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "codearts-agent", display_name: "CodeArts Agent", project_dir: ".codeartsdoer/skills", global_dir: GlobalDir::Home(".codeartsdoer/skills"), universal: false },
    AgentInfo { key: "codebuddy", display_name: "CodeBuddy", project_dir: ".codebuddy/skills", global_dir: GlobalDir::Home(".codebuddy/skills"), universal: false },
    AgentInfo { key: "codemaker", display_name: "Codemaker", project_dir: ".codemaker/skills", global_dir: GlobalDir::Home(".codemaker/skills"), universal: false },
    AgentInfo { key: "codestudio", display_name: "Code Studio", project_dir: ".codestudio/skills", global_dir: GlobalDir::Home(".codestudio/skills"), universal: false },
    AgentInfo { key: "codex", display_name: "Codex", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "command-code", display_name: "Command Code", project_dir: ".commandcode/skills", global_dir: GlobalDir::Home(".commandcode/skills"), universal: false },
    AgentInfo { key: "continue", display_name: "Continue", project_dir: ".continue/skills", global_dir: GlobalDir::Home(".continue/skills"), universal: false },
    AgentInfo { key: "cortex", display_name: "Cortex Code", project_dir: ".cortex/skills", global_dir: GlobalDir::Home(".snowflake/cortex/skills"), universal: false },
    AgentInfo { key: "crush", display_name: "Crush", project_dir: ".crush/skills", global_dir: GlobalDir::Home(".config/crush/skills"), universal: false },
    AgentInfo { key: "cursor", display_name: "Cursor", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".cursor/skills"), universal: true },
    AgentInfo { key: "deepagents", display_name: "Deep Agents", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".deepagents/agent/skills"), universal: true },
    AgentInfo { key: "devin", display_name: "Devin for Terminal", project_dir: ".devin/skills", global_dir: GlobalDir::ConfigHome("devin/skills"), universal: false },
    AgentInfo { key: "dexto", display_name: "Dexto", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "droid", display_name: "Droid", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".factory/skills"), universal: true },
    AgentInfo { key: "eve", display_name: "Eve", project_dir: "agent/skills", global_dir: GlobalDir::None, universal: false },
    AgentInfo { key: "firebender", display_name: "Firebender", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".firebender/skills"), universal: true },
    AgentInfo { key: "forgecode", display_name: "ForgeCode", project_dir: ".forge/skills", global_dir: GlobalDir::Home(".forge/skills"), universal: false },
    AgentInfo { key: "fx", display_name: "fx", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "gemini-cli", display_name: "Gemini CLI", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".gemini/skills"), universal: true },
    AgentInfo { key: "github-copilot", display_name: "GitHub Copilot", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".copilot/skills"), universal: true },
    AgentInfo { key: "goose", display_name: "Goose", project_dir: ".goose/skills", global_dir: GlobalDir::ConfigHome("goose/skills"), universal: false },
    AgentInfo { key: "grok", display_name: "Grok Build", project_dir: ".grok/skills", global_dir: GlobalDir::EnvOrHome { var: "GROK_HOME", base: ".grok", sub: "skills" }, universal: false },
    AgentInfo { key: "hermes-agent", display_name: "Hermes Agent", project_dir: ".hermes/skills", global_dir: GlobalDir::EnvOrHome { var: "HERMES_HOME", base: ".hermes", sub: "skills" }, universal: false },
    AgentInfo { key: "inference-sh", display_name: "inference.sh", project_dir: ".inferencesh/skills", global_dir: GlobalDir::Home(".inferencesh/skills"), universal: false },
    AgentInfo { key: "jazz", display_name: "Jazz", project_dir: ".jazz/skills", global_dir: GlobalDir::Home(".jazz/skills"), universal: false },
    AgentInfo { key: "junie", display_name: "Junie", project_dir: ".junie/skills", global_dir: GlobalDir::Home(".junie/skills"), universal: false },
    AgentInfo { key: "iflow-cli", display_name: "iFlow CLI", project_dir: ".iflow/skills", global_dir: GlobalDir::Home(".iflow/skills"), universal: false },
    AgentInfo { key: "kilo", display_name: "Kilo Code", project_dir: ".agents/skills", global_dir: GlobalDir::Home(".kilo/skills"), universal: true },
    AgentInfo { key: "kimchi", display_name: "Kimchi", project_dir: ".kimchi/skills", global_dir: GlobalDir::Home(".config/kimchi/harness/skills"), universal: false },
    AgentInfo { key: "kimi-code-cli", display_name: "Kimi Code CLI", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "kiro-cli", display_name: "Kiro CLI", project_dir: ".kiro/skills", global_dir: GlobalDir::Home(".kiro/skills"), universal: false },
    AgentInfo { key: "kode", display_name: "Kode", project_dir: ".kode/skills", global_dir: GlobalDir::Home(".kode/skills"), universal: false },
    AgentInfo { key: "lingma", display_name: "Lingma", project_dir: ".lingma/skills", global_dir: GlobalDir::Home(".lingma/skills"), universal: false },
    AgentInfo { key: "loaf", display_name: "Loaf", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "mcpjam", display_name: "MCPJam", project_dir: ".mcpjam/skills", global_dir: GlobalDir::Home(".mcpjam/skills"), universal: false },
    AgentInfo { key: "minimax-code", display_name: "MiniMax Code", project_dir: ".minimax/skills", global_dir: GlobalDir::Home(".minimax/skills"), universal: false },
    AgentInfo { key: "mistral-vibe", display_name: "Mistral Vibe", project_dir: ".vibe/skills", global_dir: GlobalDir::EnvOrHome { var: "VIBE_HOME", base: ".vibe", sub: "skills" }, universal: false },
    AgentInfo { key: "moxby", display_name: "Moxby", project_dir: ".moxby/skills", global_dir: GlobalDir::Home(".moxby/skills"), universal: false },
    AgentInfo { key: "mux", display_name: "Mux", project_dir: ".mux/skills", global_dir: GlobalDir::Home(".mux/skills"), universal: false },
    AgentInfo { key: "opencode", display_name: "OpenCode", project_dir: ".agents/skills", global_dir: GlobalDir::ConfigHome("opencode/skills"), universal: true },
    AgentInfo { key: "openhands", display_name: "OpenHands", project_dir: ".openhands/skills", global_dir: GlobalDir::Home(".openhands/skills"), universal: false },
    AgentInfo { key: "ona", display_name: "Ona", project_dir: ".ona/skills", global_dir: GlobalDir::Home(".ona/skills"), universal: false },
    AgentInfo { key: "pi", display_name: "Pi", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "posit-assistant", display_name: "Posit Assistant", project_dir: ".posit/assistant/skills", global_dir: GlobalDir::Home(".posit/assistant/skills"), universal: false },
    AgentInfo { key: "qoder", display_name: "Qoder", project_dir: ".qoder/skills", global_dir: GlobalDir::Home(".qoder/skills"), universal: false },
    AgentInfo { key: "qoder-cn", display_name: "Qoder CN", project_dir: ".qoder/skills", global_dir: GlobalDir::Home(".qoder-cn/skills"), universal: false },
    AgentInfo { key: "qwen-code", display_name: "Qwen Code", project_dir: ".qwen/skills", global_dir: GlobalDir::Home(".qwen/skills"), universal: false },
    AgentInfo { key: "replit", display_name: "Replit", project_dir: ".agents/skills", global_dir: GlobalDir::ConfigHome("agents/skills"), universal: true },
    AgentInfo { key: "reasonix", display_name: "Reasonix", project_dir: ".reasonix/skills", global_dir: GlobalDir::Home(".reasonix/skills"), universal: false },
    AgentInfo { key: "rovodev", display_name: "Rovo Dev", project_dir: ".rovodev/skills", global_dir: GlobalDir::Home(".rovodev/skills"), universal: false },
    AgentInfo { key: "roo", display_name: "Roo Code", project_dir: ".roo/skills", global_dir: GlobalDir::Home(".roo/skills"), universal: false },
    AgentInfo { key: "sarvam-code", display_name: "Sarvam Code", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "tabnine-cli", display_name: "Tabnine CLI", project_dir: ".tabnine/agent/skills", global_dir: GlobalDir::Home(".tabnine/agent/skills"), universal: false },
    AgentInfo { key: "terramind", display_name: "Terramind", project_dir: ".terramind/skills", global_dir: GlobalDir::Home(".terramind/skills"), universal: false },
    AgentInfo { key: "tinycloud", display_name: "Tinycloud", project_dir: ".tinycloud/skills", global_dir: GlobalDir::Home(".tinycloud/skills"), universal: false },
    AgentInfo { key: "trae", display_name: "Trae", project_dir: ".trae/skills", global_dir: GlobalDir::Home(".trae/skills"), universal: false },
    AgentInfo { key: "trae-cn", display_name: "Trae CN", project_dir: ".trae/skills", global_dir: GlobalDir::Home(".trae-cn/skills"), universal: false },
    AgentInfo { key: "warp", display_name: "Warp", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "windsurf", display_name: "Windsurf", project_dir: ".windsurf/skills", global_dir: GlobalDir::Home(".codeium/windsurf/skills"), universal: false },
    AgentInfo { key: "zed", display_name: "Zed", project_dir: ".agents/skills", global_dir: GlobalDir::Shared, universal: true },
    AgentInfo { key: "zcode", display_name: "ZCode", project_dir: ".zcode/skills", global_dir: GlobalDir::Home(".zcode/skills"), universal: false },
    AgentInfo { key: "zencoder", display_name: "Zencoder", project_dir: ".zencoder/skills", global_dir: GlobalDir::Home(".zencoder/skills"), universal: false },
    AgentInfo { key: "zenflow", display_name: "Zenflow", project_dir: ".zencoder/skills", global_dir: GlobalDir::Home(".zencoder/skills"), universal: false },
    AgentInfo { key: "neovate", display_name: "Neovate", project_dir: ".neovate/skills", global_dir: GlobalDir::Home(".neovate/skills"), universal: false },
    AgentInfo { key: "pochi", display_name: "Pochi", project_dir: ".pochi/skills", global_dir: GlobalDir::Home(".pochi/skills"), universal: false },
    AgentInfo { key: "promptscript", display_name: "PromptScript", project_dir: ".agents/skills", global_dir: GlobalDir::None, universal: true },
    AgentInfo { key: "adal", display_name: "AdaL", project_dir: ".adal/skills", global_dir: GlobalDir::Home(".adal/skills"), universal: false },
    AgentInfo { key: "universal", display_name: "Universal", project_dir: ".agents/skills", global_dir: GlobalDir::ConfigHome("agents/skills"), universal: true },
];

/// Look up an agent by CLI key.
pub fn by_key(key: &str) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.key == key)
}

/// Translate a display name from `ls --json` back to the agent entry.
pub fn by_display_name(display_name: &str) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.display_name == display_name)
}

/// Agent keys whose entries share the shared canonical dir for global scope
/// (excluding the `universal` pseudo-agent itself), used to explain that
/// removing a link does not hide a skill from these agents.
pub fn shared_global_agents() -> Vec<&'static AgentInfo> {
    AGENTS
        .iter()
        .filter(|a| a.key != "universal" && matches!(a.global_dir, GlobalDir::Shared))
        .collect()
}

/// Resolve which agent owns a directory: an agent whose global skills dir
/// equals `dir`. Used when scanning disk to attribute a skill to agents.
pub fn agent_for_global_dir(dir: &std::path::Path, home: &std::path::Path) -> Option<&'static AgentInfo> {
    AGENTS.iter().find(|a| a.global_skills_dir(home).as_deref() == Some(dir))
}

/// All agent keys — the allowlist for `-a` validation.
pub fn valid_keys() -> Vec<&'static str> {
    AGENTS.iter().map(|a| a.key).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_the_1_7_4_snapshot_size() {
        // skills@1.7.4 has exactly 79 agents. A drift here means the
        // snapshot was edited by hand and the contract tests must be re-run.
        assert_eq!(AGENTS.len(), 79);
    }

    #[test]
    fn keys_and_display_names_are_unique() {
        let mut keys: Vec<_> = AGENTS.iter().map(|a| a.key).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate agent keys");

        let mut names: Vec<_> = AGENTS.iter().map(|a| a.display_name).collect();
        names.sort_unstable();
        let n = names.len();
        names.dedup();
        assert_eq!(names.len(), n, "duplicate display names");
    }

    #[test]
    fn universal_agents_use_the_shared_project_dir() {
        for a in AGENTS {
            assert_eq!(
                a.universal,
                a.project_dir == ".agents/skills",
                "{}: universal flag and project_dir disagree",
                a.key
            );
        }
    }

    #[test]
    fn lookup_by_key_and_display_name() {
        assert_eq!(by_key("claude-code").unwrap().display_name, "Claude Code");
        assert_eq!(by_display_name("Kilo Code").unwrap().key, "kilo");
        assert!(by_key("Kilo Code").is_none(), "display name is not a key");
        assert!(by_display_name("claude-code").is_none());
        assert!(by_key("nope").is_none());
    }

    #[test]
    fn claude_honors_config_dir_env() {
        // SAFETY: unique value, restored immediately.
        std::env::set_var("CLAUDE_CONFIG_DIR", "/tmp/claude-cfg-test");
        let home = std::path::Path::new("/home/u");
        let dir = by_key("claude-code").unwrap().global_skills_dir(home);
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        assert_eq!(dir, Some(PathBuf::from("/tmp/claude-cfg-test/skills")));
    }

    #[test]
    fn global_dirs_resolve_against_home() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(
            by_key("cursor").unwrap().global_skills_dir(home),
            Some(home.join(".cursor/skills"))
        );
        assert_eq!(
            by_key("codex").unwrap().global_skills_dir(home),
            Some(home.join(".agents/skills"))
        );
        assert_eq!(by_key("eve").unwrap().global_skills_dir(home), None);
        assert_eq!(by_key("promptscript").unwrap().global_skills_dir(home), None);
    }

    #[test]
    fn config_home_respects_xdg_env() {
        // SAFETY: unique value, restored immediately.
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        let home = std::path::Path::new("/home/u");
        let dir = by_key("amp").unwrap().global_skills_dir(home);
        std::env::remove_var("XDG_CONFIG_HOME");
        assert_eq!(dir, Some(PathBuf::from("/tmp/xdg-test/agents/skills")));
    }

    #[test]
    fn only_eve_and_promptscript_lack_global_scope() {
        let no_global: Vec<_> = AGENTS
            .iter()
            .filter(|a| !a.supports_global())
            .map(|a| a.key)
            .collect();
        assert_eq!(no_global, vec!["eve", "promptscript"]);
    }

    #[test]
    fn agent_for_global_dir_finds_owner() {
        let home = std::path::Path::new("/home/u");
        assert_eq!(
            agent_for_global_dir(&home.join(".cursor/skills"), home).unwrap().key,
            "cursor"
        );
        // Shared dir resolves to the first agent declaring it.
        assert!(agent_for_global_dir(&home.join(".agents/skills"), home).is_some());
    }
}
