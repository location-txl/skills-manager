import type { AgentStatus, CatalogResult } from "./api";

/** Hiding an agent is display-only: its icons, filter entries and dialog
 *  targets go away, but every skill stays in the library — no skill is ever
 *  removed or unlinked because of a hidden agent. */
export const HIDDEN_AGENTS_KEY = "skills-manager.hiddenAgentIds";

export function readHiddenAgentIds(): Set<string> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(HIDDEN_AGENTS_KEY) ?? "[]");
    return new Set(
      Array.isArray(value)
        ? value.filter((id): id is string => typeof id === "string" && id.trim().length > 0)
        : [],
    );
  } catch {
    return new Set<string>();
  }
}

export interface AgentOption {
  id: string;
  name: string;
}

/** Agents Settings must keep a switch for. Being detected on disk is not the
 *  whole story: the CLI also attributes skills to agents that own no skills
 *  directory of their own (Kilo Code is "installed" via ~/.kilocode and reads
 *  the shared .agents/skills dir), so anything the catalogs attribute — or
 *  that is already hidden — stays manageable, unknown IDs included. */
export function manageableAgentOptions(
  allAgents: AgentStatus[],
  hiddenAgentIds: ReadonlySet<string>,
  catalogs: ReadonlyArray<CatalogResult | null | undefined>,
): AgentOption[] {
  // name from the catalog wins for ids the agent map does not know.
  const attributed = new Map<string, string>();
  for (const catalog of catalogs) {
    for (const skill of catalog?.skills ?? []) {
      for (const agent of skill.agents) {
        if (!attributed.has(agent.id)) attributed.set(agent.id, agent.name);
      }
    }
  }
  const options: AgentOption[] = [];
  const included = new Set<string>();
  const push = (id: string, name: string) => {
    if (included.has(id)) return;
    included.add(id);
    options.push({ id, name });
  };
  for (const agent of allAgents) {
    if (agent.detected || hiddenAgentIds.has(agent.id) || attributed.has(agent.id)) {
      push(agent.id, agent.name);
    }
  }
  for (const [id, name] of attributed) push(id, name);
  for (const id of hiddenAgentIds) push(id, id);
  return options;
}
