// @vitest-environment jsdom
import { act } from "react";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() },
}));
vi.mock("../lib/api", () => ({
  shareSkill: vi.fn(),
  unshareSkill: vi.fn(),
}));

const AGENTS = [
  {
    id: "claude-code",
    name: "Claude Code",
    detected: true,
    supportsGlobal: true,
    globalDir: "/home/.claude/skills",
    projectDir: ".claude/skills",
  },
  {
    id: "codex",
    name: "Codex",
    detected: true,
    supportsGlobal: true,
    // Same dir the runtime reports as the shared one — locked.
    globalDir: "/home/.agents/skills",
    projectDir: ".agents/skills",
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    detected: true,
    supportsGlobal: true,
    globalDir: "/home/.gemini/skills",
    projectDir: ".agents/skills",
  },
  {
    id: "cursor",
    name: "Cursor",
    detected: true,
    supportsGlobal: true,
    globalDir: "/home/.cursor/skills",
    projectDir: ".agents/skills",
  },
  {
    // Not detected and does not have the skill — not shown at all.
    id: "kilo",
    name: "Kilo Code",
    detected: false,
    supportsGlobal: true,
    globalDir: "/home/.kilo/skills",
    projectDir: ".agents/skills",
  },
];

const refreshAll = vi.fn();
/** Swappable in tests (the fallback case needs an empty relevant set). */
let mockAgents = AGENTS;

vi.mock("../context/AppContext", () => ({
  useApp: () => ({
    runtime: { globalSkillsDir: "/home/.agents/skills" },
    agents: mockAgents,
    allAgents: mockAgents,
    refreshAll,
  }),
}));

import type { Scope, SkillView } from "../lib/api";
import { shareSkill, unshareSkill } from "../lib/api";
import { toast } from "sonner";

const { AgentShareBar } = await import("../components/AgentShareBar");

type AgentShareBarProps = Parameters<typeof AgentShareBar>[0];

function makeSkill(agents: SkillView["agents"]): SkillView {
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
    agents,
    isTrackedByVercel: true,
  } as SkillView;
}

/** The skill is seen by codex (shared dir, locked), gemini (symlink) and
 *  cursor (copy); claude-code is detected but does not have it. */
const SKILL = makeSkill([
  { id: "codex", name: "Codex", exposure: "shared" },
  { id: "gemini-cli", name: "Gemini CLI", exposure: "symlink" },
  { id: "cursor", name: "Cursor", exposure: "copy" },
]);

function render(props?: Partial<AgentShareBarProps>) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
  createRoot(container).render(
    createElement(AgentShareBar, {
      skill: SKILL,
      scope: { kind: "global" } as Scope,
      ...props,
    }),
  );
  return container;
}

let container: HTMLElement | null = null;

beforeEach(() => {
  container?.remove();
  container = null;
  mockAgents = AGENTS;
  vi.clearAllMocks();
});

const buttonByTitle = (titlePart: string) =>
  [...container!.querySelectorAll("button")].find((b) =>
    b.title.includes(titlePart),
  )!;

const pill = () => buttonByTitle("agentShareBar.shareAllTitle");

describe("AgentShareBar", () => {
  it("shows only detected-or-having agents with gray / colored / locked states", () => {
    act(() => {
      container = render();
    });

    // Undetected agent without the skill is not in the strip.
    expect(container!.textContent).not.toContain("Kilo Code");
    // Gray: detected, no exposure.
    const gray = buttonByTitle("Claude Code · agentShareBar.notShared");
    expect(gray.className).toContain("grayscale");
    // Colored: has the skill, no filter.
    expect(buttonByTitle("Gemini CLI · exposure.symlink").className).not.toContain("grayscale");
    // Locked: shared-dir agent carries the note and the help cursor.
    const locked = buttonByTitle("shareDialog.universalNote");
    expect(locked.title).toContain("Codex");
    expect(locked.className).toContain("cursor-help");
    // Fixed order: plain name sort — exposure never moves an icon.
    const titles = [...container!.querySelectorAll("button")].map((b) => b.title);
    const iconTitles = titles.filter((t) => t !== "agentShareBar.shareAllTitle");
    expect(iconTitles.map((t) => t.split(" · ")[0])).toEqual([
      "Claude Code",
      "Codex",
      "Cursor",
      "Gemini CLI",
    ]);
  });

  it("locked and copy clicks explain instead of calling the backend", () => {
    act(() => {
      container = render();
    });

    act(() => {
      buttonByTitle("shareDialog.universalNote").click();
      buttonByTitle("Cursor · exposure.copy").click();
    });
    expect(toast.info).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.lockedHint"),
    );
    expect(toast.info).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.managedHint"),
    );
    expect(shareSkill).not.toHaveBeenCalled();
    expect(unshareSkill).not.toHaveBeenCalled();
  });

  it("clicking a gray icon shares to that single agent and refreshes", async () => {
    (shareSkill as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      { agent: "Claude Code", status: "linked", detail: "/home/.claude/skills/web-design" },
    ]);
    act(() => {
      container = render();
    });

    await act(async () => {
      buttonByTitle("Claude Code · agentShareBar.notShared").click();
    });

    expect(shareSkill).toHaveBeenCalledWith({
      path: "/home/.agents/skills/web-design",
      scope: { kind: "global" },
      agents: ["claude-code"],
      replaceCopies: false,
    });
    expect(toast.success).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.sharedWith"),
    );
    expect(refreshAll).toHaveBeenCalled();
  });

  it("a backend conflict is pointed at the detail page, never auto-replaced", async () => {
    (shareSkill as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      { agent: "Claude Code", status: "conflict", detail: "a real folder already exists here" },
    ]);
    act(() => {
      container = render();
    });

    await act(async () => {
      buttonByTitle("Claude Code · agentShareBar.notShared").click();
    });

    expect(toast.warning).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.conflictHint"),
      expect.objectContaining({ description: "a real folder already exists here" }),
    );
    // No confirmed re-run ever happens from the card.
    expect(shareSkill).toHaveBeenCalledTimes(1);
  });

  it("clicking a symlinked agent removes its link", async () => {
    (unshareSkill as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      { agent: "Gemini CLI", status: "removed", detail: "/home/.gemini/skills/web-design" },
    ]);
    act(() => {
      container = render();
    });

    await act(async () => {
      buttonByTitle("Gemini CLI · exposure.symlink").click();
    });

    expect(unshareSkill).toHaveBeenCalledWith({
      path: "/home/.agents/skills/web-design",
      scope: { kind: "global" },
      agents: ["gemini-cli"],
    });
    expect(toast.success).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.unsharedFrom"),
    );
  });

  it("the all pill shares to every gray unlocked agent in one call", async () => {
    (shareSkill as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      { agent: "Claude Code", status: "linked", detail: null },
    ]);
    act(() => {
      container = render();
    });

    await act(async () => {
      pill().click();
    });

    expect(shareSkill).toHaveBeenCalledTimes(1);
    expect(shareSkill).toHaveBeenCalledWith(
      expect.objectContaining({ agents: ["claude-code"], replaceCopies: false }),
    );
    expect(toast.success).toHaveBeenCalledWith(
      expect.stringContaining("agentShareBar.shareAllDone"),
    );
  });

  it("the strip is disabled while an operation is pending", async () => {
    let resolveOp: (items: unknown[]) => void = () => {};
    (shareSkill as unknown as ReturnType<typeof vi.fn>).mockReturnValue(
      new Promise((resolve) => {
        resolveOp = resolve;
      }),
    );
    act(() => {
      container = render();
    });

    await act(async () => {
      buttonByTitle("Claude Code · agentShareBar.notShared").click();
    });
    expect((pill() as HTMLButtonElement).disabled).toBe(true);
    expect(
      (buttonByTitle("Gemini CLI · exposure.symlink") as HTMLButtonElement).disabled,
    ).toBe(true);

    await act(async () => {
      resolveOp([{ agent: "Claude Code", status: "linked", detail: null }]);
    });
    expect((pill() as HTMLButtonElement).disabled).toBe(false);
  });

  it("falls back to the not-linked note when no agent is relevant", () => {
    // Only an undetected agent without the skill — nothing to show.
    mockAgents = [AGENTS[4]];
    act(() => {
      container = render({ skill: makeSkill([]) });
    });
    expect(container!.textContent).toContain("agents.notLinked");
    expect(container!.querySelectorAll("button").length).toBe(0);
  });
});
