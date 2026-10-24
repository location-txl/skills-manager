// @vitest-environment jsdom
import { act } from "react";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const { SkillStatusPill } = await import("../components/SkillStatusPill");
const { AgentChips } = await import("../components/AgentChips");

function render(element: ReturnType<typeof createElement>) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
  createRoot(container).render(element);
  return container;
}

let container: HTMLElement | null = null;

beforeEach(() => {
  container?.remove();
  container = null;
});

describe("SkillStatusPill", () => {
  it("renders the translation key per status", () => {
    const cases: Array<[string, string]> = [
      ["installed", "status.installed"],
      ["untracked", "status.untracked"],
      ["broken-link", "status.brokenLink"],
      ["metadata-unsupported", "status.metadataUnsupported"],
    ];
    for (const [status, key] of cases) {
      act(() => {
        container = render(createElement(SkillStatusPill, { status: status as never }));
      });
      expect(container!.textContent).toBe(key);
    }
  });
});

describe("AgentChips", () => {
  it("shows the notLinked hint when no agent sees the skill", () => {
    act(() => {
      container = render(createElement(AgentChips, { agents: [] }));
    });
    expect(container!.textContent).toContain("agents.notLinked");
  });

  it("lists agents and counts the overflow", () => {
    const agents = [1, 2, 3, 4, 5].map((i) => ({
      id: `agent-${i}`,
      name: `Agent ${i}`,
      exposure: "symlink" as const,
    }));
    act(() => {
      container = render(createElement(AgentChips, { agents, max: 3 }));
    });
    expect(container!.textContent).toContain("Agent 1");
    expect(container!.textContent).toContain("Agent 3");
    expect(container!.textContent).not.toContain("Agent 4");
    expect(container!.textContent).toContain("+2");
  });
});
