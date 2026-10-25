// @vitest-environment jsdom
import { act } from "react";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));

const refreshAll = vi.fn();
/** One detected agent so the share bar has something to render. */
let mockAgents: unknown[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    detected: true,
    supportsGlobal: true,
    globalDir: "/home/.claude/skills",
    projectDir: ".claude/skills",
  },
];

vi.mock("../context/AppContext", () => ({
  useApp: () => ({
    runtime: { globalSkillsDir: "/home/.agents/skills" },
    agents: mockAgents,
    allAgents: mockAgents,
    hiddenAgentIds: new Set<string>(),
    refreshAll,
  }),
}));

import type { SkillView } from "../lib/api";

const { SkillCard } = await import("../components/SkillCard");

type SkillCardProps = Parameters<typeof SkillCard>[0];

function makeSkill(overrides: Partial<SkillView> = {}): SkillView {
  return {
    id: "web-design",
    slug: "web-design",
    name: "web-design",
    description: null,
    scope: "global",
    path: "/home/.agents/skills/web-design",
    status: "installed",
    source: null,
    sourceUrl: null,
    sourceType: null,
    installedAt: null,
    updatedAt: null,
    agents: [{ id: "claude-code", name: "Claude Code", exposure: "symlink" }],
    isTrackedByVercel: true,
    ...overrides,
  } as SkillView;
}

function render(props: Partial<SkillCardProps> = {}) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
  createRoot(container).render(
    createElement(SkillCard, {
      skill: makeSkill(),
      onOpen: onOpen,
      ...props,
    }),
  );
  return container;
}

const onOpen = vi.fn();
const onToggleSelect = vi.fn();

let container: HTMLElement | null = null;
const defaultAgents = mockAgents;

beforeEach(() => {
  container?.remove();
  container = null;
  mockAgents = defaultAgents;
  onOpen.mockClear();
  onToggleSelect.mockClear();
  vi.clearAllMocks();
});

const root = () => container!.firstElementChild as HTMLElement;

describe("SkillCard", () => {
  it("is a div with button semantics and opens the detail on click and Enter", () => {
    act(() => {
      container = render();
    });
    const card = root();
    expect(card.tagName).toBe("DIV");
    expect(card.getAttribute("role")).toBe("button");

    act(() => {
      card.click();
    });
    expect(onOpen).toHaveBeenCalledTimes(1);

    act(() => {
      card.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
      );
    });
    expect(onOpen).toHaveBeenCalledTimes(2);
  });

  it("Enter on a nested element does not open the detail", () => {
    act(() => {
      container = render();
    });
    // The share strip's keydown handler stops propagation; even without it,
    // the card only reacts to keys on itself.
    const inner = container!.querySelector("p")!;
    act(() => {
      inner.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Enter", bubbles: true }),
      );
    });
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("always shows the share bar, not just on hover", () => {
    act(() => {
      container = render();
    });
    expect(container!.textContent).toContain("agentShareBar.all");
    // No hover cross-fade slot anymore — the share bar is the only child.
    expect(container!.querySelector(".grid")).toBeNull();
    expect(container!.textContent).not.toContain("Claude Code");
  });

  it("keeps plain chips in selection mode and toggles selection instead", () => {
    act(() => {
      container = render({
        selectionMode: true,
        selected: true,
        onToggleSelect,
      });
    });
    expect(container!.textContent).not.toContain("agentShareBar.all");
    act(() => {
      root().click();
    });
    expect(onToggleSelect).toHaveBeenCalledTimes(1);
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("keeps plain chips for a broken link", () => {
    act(() => {
      container = render({ skill: makeSkill({ status: "broken-link" }) });
    });
    expect(container!.textContent).not.toContain("agentShareBar.all");
  });
});
