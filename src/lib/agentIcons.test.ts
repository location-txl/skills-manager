import { describe, it, expect } from "vitest";
import { getAgentIconSrc, agentIconNeedsDarkInvert, hasAgentIcon } from "./agentIcons";

describe("agent icon key resolution", () => {
  it("resolves hyphenated agent ids to underscore icon keys", () => {
    expect(getAgentIconSrc("claude-code")).toBe("/agent-icons/claude_code.svg");
    expect(getAgentIconSrc("gemini-cli")).toBe("/agent-icons/gemini_cli.svg");
    expect(hasAgentIcon("github-copilot")).toBe(true);
  });

  it("applies the explicit alias table for irregular ids", () => {
    expect(getAgentIconSrc("kilo")).toBe("/agent-icons/kilo_code.svg");
    expect(getAgentIconSrc("kimi-code-cli")).toBe("/agent-icons/kimi.svg");
    expect(getAgentIconSrc("roo")).toBe("/agent-icons/roo_code.svg");
    expect(getAgentIconSrc("kiro-cli")).toBe("/agent-icons/kiro.svg");
    expect(getAgentIconSrc("qoder-cn")).toBe("/agent-icons/qoder.svg");
  });

  it("falls back to null for agents without an icon asset", () => {
    expect(getAgentIconSrc("zed")).toBeNull();
    expect(hasAgentIcon("no-such-agent")).toBe(false);
  });

  it("checks dark inversion through the resolved key", () => {
    expect(agentIconNeedsDarkInvert("codex")).toBe(true);
    expect(agentIconNeedsDarkInvert("roo")).toBe(true);
    expect(agentIconNeedsDarkInvert("claude-code")).toBe(false);
  });
});
