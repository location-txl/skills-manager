import type { AgentExposure, AgentStatus, Scope, SkillView } from "./api";

/** How a listed agent already sees the skill, if at all.
 *
 *  Matches by id or display name: the catalog reports the CLI display name
 *  as `AgentExposure.id` when the agent is not in the controlled map. */
export function exposureOf(
  skill: SkillView,
  agent: AgentStatus,
): AgentExposure["exposure"] | undefined {
  return skill.agents.find((e) => e.id === agent.id || e.name === agent.name)?.exposure;
}

/** Agents whose skills dir IS the shared dir read it directly; a link there
 *  would expose the skill to every universal agent at once, so they are
 *  locked out of per-agent share/unshare entirely. */
export function isSharedDirAgent(
  agent: AgentStatus,
  scope: Scope,
  globalSkillsDir: string | null,
): boolean {
  return scope.kind === "global"
    ? !!agent.globalDir && agent.globalDir === globalSkillsDir
    : agent.projectDir === ".agents/skills";
}
