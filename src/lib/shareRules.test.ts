import { describe, it, expect } from "vitest";
import { exposureOf, isSharedDirAgent } from "./shareRules";
import type { AgentStatus, SkillView } from "./api";

const agent = (over: Partial<AgentStatus>): AgentStatus => ({
  id: "claude-code",
  name: "Claude Code",
  detected: true,
  supportsGlobal: true,
  globalDir: "/home/.claude/skills",
  projectDir: ".claude/skills",
  ...over,
});

const skill = {
  agents: [
    { id: "claude-code", name: "Claude Code", exposure: "symlink" },
    // Not in the controlled map: exposure reported under the display name.
    { id: "Some Tool", name: "Some Tool", exposure: "copy" },
  ],
} as SkillView;

describe("exposureOf", () => {
  it("matches by id", () => {
    expect(exposureOf(skill, agent({}))).toBe("symlink");
  });

  it("matches by display name when the id is unknown", () => {
    expect(exposureOf(skill, agent({ id: "other", name: "Some Tool" }))).toBe("copy");
  });

  it("returns undefined for an agent that does not see the skill", () => {
    expect(exposureOf(skill, agent({ id: "codex", name: "Codex" }))).toBeUndefined();
  });
});

describe("isSharedDirAgent", () => {
  it("locks global agents whose dir is the shared dir", () => {
    expect(
      isSharedDirAgent(agent({ globalDir: "/home/.agents/skills" }), { kind: "global" }, "/home/.agents/skills"),
    ).toBe(true);
    expect(
      isSharedDirAgent(agent({}), { kind: "global" }, "/home/.agents/skills"),
    ).toBe(false);
  });

  it("locks project agents reading the project shared dir", () => {
    expect(
      isSharedDirAgent(agent({ projectDir: ".agents/skills" }), { kind: "project", root: "/p" }, null),
    ).toBe(true);
    expect(
      isSharedDirAgent(agent({}), { kind: "project", root: "/p" }, null),
    ).toBe(false);
  });
});
