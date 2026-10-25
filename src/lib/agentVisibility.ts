import type { CatalogResult, SkillView } from "./api";

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
    return new Set();
  }
}

export function isSkillVisible(skill: SkillView, hiddenAgentIds: ReadonlySet<string>): boolean {
  return skill.agents.length === 0 || skill.agents.some((agent) => !hiddenAgentIds.has(agent.id));
}

/** Keep the full exposures on retained skills for sharing and conflict checks. */
export function visibleCatalog(
  catalog: CatalogResult | null,
  hiddenAgentIds: ReadonlySet<string>,
): CatalogResult | null {
  if (!catalog || hiddenAgentIds.size === 0) return catalog;
  return { ...catalog, skills: catalog.skills.filter((skill) => isSkillVisible(skill, hiddenAgentIds)) };
}
