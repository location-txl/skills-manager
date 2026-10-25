// @vitest-environment jsdom
import { act } from "react";
import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));
vi.mock("../lib/api", () => ({
  shareSkill: vi.fn(),
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
    // Same dir the runtime reports as the shared one.
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
];

vi.mock("../context/AppContext", () => ({
  useApp: () => ({
    runtime: { globalSkillsDir: "/home/.agents/skills" },
    agents: AGENTS,
  }),
}));

import type { Scope, SkillView } from "../lib/api";
import { shareSkill } from "../lib/api";

const { ShareDialog } = await import("../components/ShareDialog");

type ShareDialogProps = Parameters<typeof ShareDialog>[0];

function makeSkill(agents: SkillView["agents"]): SkillView {
  return {
    id: "web-design",
    slug: "web-design",
    name: "web-design",
    description: null,
    scope: "global",
    path: "/home/.cursor/skills/web-design",
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

function render(props: ShareDialogProps) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;
  createRoot(container).render(createElement(ShareDialog, props));
  return container;
}

let container: HTMLElement | null = null;

beforeEach(() => {
  container?.remove();
  container = null;
  vi.clearAllMocks();
});

const baseProps = (skill: SkillView): ShareDialogProps => ({
  open: true,
  skill,
  scope: { kind: "global" } as Scope,
  onClose: () => {},
  onChanged: () => {},
});

const buttonByText = (text: string) =>
  [...container!.querySelectorAll("button")].find((b) =>
    b.textContent?.includes(text),
  )!;

describe("ShareDialog", () => {
  it("marks agents that already see the skill and disables shared-dir agents", () => {
    const skill = makeSkill([
      { id: "gemini-cli", name: "Gemini CLI", exposure: "copy" },
    ]);
    act(() => {
      container = render(baseProps(skill));
    });

    expect(container!.textContent).toContain("shareDialog.title");
    expect(container!.textContent).toContain("shareDialog.hint");
    // Gemini holds a copy — labelled with its exposure, not preselected.
    expect(container!.textContent).toContain("exposure.copy");
    expect(buttonByText("Gemini CLI").className).not.toContain("border-accent-border");
    // Codex reads the shared dir directly — disabled with the note.
    const codex = buttonByText("Codex");
    expect((codex as HTMLButtonElement).disabled).toBe(true);
    expect(codex.textContent).toContain("shareDialog.universalNote");
    // Claude Code is a normal selectable, preselected by default.
    expect(buttonByText("Claude Code").className).toContain("border-accent-border");
  });

  it("asks for confirmation before replacing an agent's real copy", async () => {
    const skill = makeSkill([
      { id: "gemini-cli", name: "Gemini CLI", exposure: "copy" },
    ]);
    act(() => {
      container = render(baseProps(skill));
    });

    // Select the agent that holds a real copy, then hit Share.
    act(() => {
      buttonByText("Gemini CLI").click();
    });
    await act(async () => {
      buttonByText("shareDialog.action").click();
    });

    expect(container!.textContent).toContain("shareDialog.conflictTitle");
    expect(container!.textContent).toContain("Gemini CLI");
    expect(shareSkill).not.toHaveBeenCalled();
  });

  it("offers a retry confirmation when the backend reports a conflict", async () => {
    // A link to elsewhere carries no exposure for this skill, so the
    // pre-check sees nothing — only the backend can surface it.
    const skill = makeSkill([]);
    (shareSkill as unknown as ReturnType<typeof vi.fn>).mockResolvedValue([
      {
        agent: "Gemini CLI",
        status: "conflict",
        detail: "a link to another folder already exists here; confirm replacing it",
      },
    ]);
    act(() => {
      container = render(baseProps(skill));
    });

    expect(buttonByText("Gemini CLI").className).toContain("border-accent-border");
    await act(async () => {
      buttonByText("shareDialog.action").click();
    });

    expect(container!.textContent).toContain("shareDialog.retryTitle");
    expect(container!.textContent).toContain("Gemini CLI");
  });
});
