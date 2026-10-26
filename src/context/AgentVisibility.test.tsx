// @vitest-environment jsdom
import { act, useEffect, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentStatus, CatalogResult, SkillView } from "../lib/api";
import * as api from "../lib/api";
import { HIDDEN_AGENTS_KEY, readHiddenAgentIds } from "../lib/agentVisibility";
import { AppProvider, useApp } from "./AppContext";
import { Settings } from "../views/Settings";
import { Library } from "../views/Library";
import { ProjectView } from "../views/ProjectView";
import { Dashboard } from "../views/Dashboard";
import { AgentsView } from "../views/AgentsView";
import { InstallDialog } from "../components/InstallDialog";
import { ShareDialog } from "../components/ShareDialog";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { name?: string; count?: number }) =>
      `${key}${options?.name ? ` ${options.name}` : ""}${options?.count !== undefined ? ` ${options.count}` : ""}`,
    i18n: { changeLanguage: vi.fn() },
  }),
}));
vi.mock("../lib/api", async (original) => ({
  ...await original<typeof api>(),
  getRuntimeStatus: vi.fn(), listSkills: vi.fn(), listAgentStatus: vi.fn(),
  getLastCliError: vi.fn(async () => null), openProject: vi.fn(),
  installSkill: vi.fn(), shareSkill: vi.fn(), removeSkill: vi.fn(),
  readSkillDoc: vi.fn(async () => ({ markdown: "# Skill", truncated: false })),
}));

const agents: AgentStatus[] = [
  { id: "claude-code", name: "Claude Code", detected: true, supportsGlobal: true, globalDir: "/home/.claude/skills", projectDir: ".claude/skills" },
  { id: "codex", name: "Codex", detected: true, supportsGlobal: true, globalDir: "/home/.agents/skills", projectDir: ".agents/skills" },
  // Same directory as Claude, used to ensure hidden co-owners still count.
  { id: "co-owner", name: "Co-owner", detected: true, supportsGlobal: true, globalDir: "/home/.claude/skills", projectDir: ".claude/skills" },
];

function skill(id: string, agentIds: string[], scope: "global" | "project" = "global"): SkillView {
  return {
    id, slug: id, name: id, description: null, scope, path: `/skills/${id}`,
    status: "installed", source: null, sourceUrl: null, sourceType: null,
    installedAt: null, updatedAt: null, isTrackedByVercel: true,
    agents: agentIds.map((id) => ({ id, name: agents.find((a) => a.id === id)!.name, exposure: "symlink" })),
  };
}
const skills = [skill("hidden-only", ["claude-code"]), skill("mixed", ["claude-code", "codex"]), skill("unlinked", [])];
const catalog: CatalogResult = { skills, diagnostics: [], lockState: "ok", cliAvailable: true };

let app: ReturnType<typeof useApp>;
function Probe() {
  const value = useApp();
  useEffect(() => { app = value; }, [value]);
  return null;
}
let container: HTMLDivElement;
let root: Root;

async function render(children: ReactNode = null) {
  await act(async () => {
    root.render(<AppProvider><MemoryRouter><Probe />{children}</MemoryRouter></AppProvider>);
  });
}
async function click(button: Element | undefined | null) {
  expect(button).toBeTruthy();
  await act(async () => { (button as HTMLButtonElement).click(); });
}
function button(text: string) {
  return [...container.querySelectorAll("button")].find((b) => b.textContent?.includes(text));
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  localStorage.clear();
  vi.clearAllMocks();
  vi.mocked(api.getRuntimeStatus).mockResolvedValue({
    nodePath: "/node", nodeVersion: "22", cliPath: "/cli", cliVersion: "1.7.4",
    cliSource: null, writesSupported: true, degraded: false,
    globalSkillsDir: "/home/.agents/skills", globalLockPath: "/lock", notes: [],
  });
  vi.mocked(api.listAgentStatus).mockResolvedValue(agents);
  vi.mocked(api.listSkills).mockImplementation(async (scope) => ({
    ...catalog, skills: skills.map((s) => ({ ...s, scope: scope.kind })),
  }));
  vi.mocked(api.openProject).mockResolvedValue({ root: "/project", name: "Project", hasLock: true, skillsDirExists: true });
  vi.mocked(api.installSkill).mockResolvedValue({ success: true, exitCode: 0, message: "ok", stdout: null, stderr: null, items: [], cancelled: false, timedOut: false });
  vi.mocked(api.shareSkill).mockResolvedValue([]);
  vi.mocked(api.removeSkill).mockResolvedValue({ success: true, exitCode: 0, message: "ok", stdout: null, stderr: null, items: [], cancelled: false, timedOut: false });
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
});

describe("Agent visibility", () => {
  it.each(["invalid json", "null", "{}", '"codex"'])("ignores invalid persisted preferences: %s", (value) => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, value);
    expect([...readHiddenAgentIds()]).toEqual([]);
  });

  it("deduplicates IDs and ignores invalid entries", () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["codex", 1, null, "", " ", "codex"]');
    expect([...readHiddenAgentIds()]).toEqual(["codex"]);
  });

  it("persists toggles; no skill ever leaves the catalogs themselves", async () => {
    await render();
    await act(async () => { await app.openProject("/project"); });
    await act(async () => { app.openSkillDetail(skills[0]); app.toggleAgentVisibility("claude-code"); });
    expect(app.allAgents).toEqual(agents);
    expect(app.agents.map((a) => a.id)).toEqual(["codex", "co-owner"]);
    for (const c of [app.globalCatalog, app.projectCatalog]) {
      expect(c!.skills.map((s) => s.id)).toEqual(["hidden-only", "mixed", "unlinked"]);
      expect(c!.skills.find((s) => s.id === "mixed")!.agents).toHaveLength(2);
    }
    expect(app.detailSkill).toBe(skills[0]);
    expect(JSON.parse(localStorage.getItem(HIDDEN_AGENTS_KEY)!)).toEqual(["claude-code"]);
    await act(async () => { await app.refreshAll(); });
    expect(app.globalCatalog!.skills).toHaveLength(3);
    await act(async () => { app.toggleAgentVisibility("claude-code"); });
    expect(app.globalCatalog!.skills).toHaveLength(3);
    await act(async () => { agents.forEach((a) => app.toggleAgentVisibility(a.id)); });
    expect(app.agents).toEqual([]);
    expect(app.globalCatalog!.skills.map((s) => s.id)).toEqual(["hidden-only", "mixed", "unlinked"]);
  });

  it("loads persisted preferences before displaying catalogs and keeps recovery switches", async () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["claude-code", "retired-agent"]');
    vi.mocked(api.listAgentStatus).mockResolvedValue(agents.map((a) => ({ ...a, detected: false })));
    await render(<Settings />);
    expect(app.globalCatalog!.skills).toHaveLength(3);
    const toggle = container.querySelector('[role="switch"][aria-label="settings.showAgent Claude Code"]');
    expect(toggle?.getAttribute("aria-checked")).toBe("false");
    expect(container.querySelector('[aria-label="settings.showAgent retired-agent"]')).not.toBeNull();
    await click(toggle);
    expect(app.hiddenAgentIds.has("claude-code")).toBe(false);
    expect(app.globalCatalog!.skills).toHaveLength(3);
  });

  it("keeps a switch for CLI-attributed agents that own no skills directory", async () => {
    // Kilo Code on a real machine: the CLI sees ~/.kilocode and the shared
    // .agents/skills dir, so it attributes skills to it — but there is no
    // ~/.kilo/skills, so disk detection alone never lists it in Settings.
    const kilo: AgentStatus = { id: "kilo", name: "Kilo Code", detected: false, supportsGlobal: true, globalDir: "/home/.kilo/skills", projectDir: ".agents/skills" };
    vi.mocked(api.listAgentStatus).mockResolvedValue([...agents, kilo]);
    const attributed = {
      ...skill("shared-dir-skill", ["codex"]),
      agents: [
        { id: "codex", name: "Codex", exposure: "symlink" as const },
        { id: "kilo", name: "Kilo Code", exposure: "unknown" as const },
      ],
    };
    vi.mocked(api.listSkills).mockImplementation(async (scope) => ({
      ...catalog,
      skills: [...skills, { ...attributed, scope: scope.kind }],
    }));
    await render(<Settings />);
    const toggle = container.querySelector('[role="switch"][aria-label="settings.showAgent Kilo Code"]');
    expect(toggle?.getAttribute("aria-checked")).toBe("true");
    await click(toggle);
    expect(app.hiddenAgentIds.has("kilo")).toBe(true);
    expect(toggle?.getAttribute("aria-checked")).toBe("false");
    await render(<Library />);
    expect(container.textContent).toContain("shared-dir-skill");
    expect(container.textContent).not.toContain("Kilo Code");
    expect([...container.querySelectorAll<HTMLSelectElement>("select option")].some((o) => o.value === "kilo")).toBe(false);
  });

  it("keeps skills listed while any exposing agent is visible, in both scopes", async () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["claude-code"]');
    await render(<Library />);
    // "hidden-only" is Claude-only → filtered; "mixed" keeps Codex; "unlinked"
    // has no agent exposure and is never affected by hiding.
    expect(container.textContent).not.toContain("hidden-only");
    expect(container.textContent).toContain("mixed");
    expect(container.textContent).toContain("unlinked");
    await act(async () => { app.toggleAgentVisibility("claude-code"); });
    expect(container.textContent).toContain("hidden-only");
    await act(async () => {
      app.toggleAgentVisibility("claude-code");
      app.toggleAgentVisibility("codex");
      app.toggleAgentVisibility("co-owner");
    });
    expect(container.textContent).not.toContain("hidden-only");
    expect(container.textContent).not.toContain("mixed");
    expect(container.textContent).toContain("unlinked");
  });

  it("clears hidden agent filters and prunes now-invisible skills from the selection", async () => {
    await render(<Library />);
    const select = container.querySelector("select")!;
    await act(async () => {
      select.value = "claude-code";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await click(button("common.select"));
    await click(button("common.selectAll"));
    await act(async () => { app.toggleAgentVisibility("claude-code"); });
    expect(select.value).toBe("");
    expect([...select.options].some((o) => o.value === "claude-code")).toBe(false);
    // Hiding Claude Code drops "hidden-only" from the list and from the
    // selection; "mixed" stays visible (Codex) and keeps its selection — the
    // uninstall must not act on rows the user can no longer see.
    expect(container.textContent).not.toContain("hidden-only");
    expect(container.textContent).toContain("mixed");
    expect(container.textContent).not.toContain("Claude Code");
    await click(button("common.uninstall"));
    // ConfirmDialog's explicit confirmation is the second uninstall button.
    await click([...container.querySelectorAll("button")].reverse().find((b) => b.textContent === "common.uninstall"));
    expect(api.removeSkill).toHaveBeenCalledWith(expect.objectContaining({ names: ["mixed"] }));
  });

  it("hides agent rows and chips but keeps skill counts over the full catalogs", async () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["claude-code", "co-owner"]');
    await render(<><Dashboard /><AgentsView /><ProjectView /></>);
    await act(async () => { await app.openProject("/project"); });
    // "hidden-only" (Claude Code only) leaves the project list; "mixed"
    // (Codex still visible) stays. Counts keep spanning the full catalog.
    expect(container.textContent).not.toContain("hidden-only");
    expect(container.textContent).toContain("mixed");
    expect(container.textContent).not.toContain("Claude Code");
    expect(container.textContent).toContain("project.skillCount 3");
    const stat = [...container.querySelectorAll('a[href="/library"]')][0];
    expect(stat.textContent).toContain("3");
    const agentStat = container.querySelector('a[href="/agents"]')!;
    expect(agentStat.textContent).toContain("1");
  });

  it("excludes hidden install targets for defaults and select all", async () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["claude-code"]');
    await render(<InstallDialog open source="owner/repo" onClose={() => {}} onInstalled={() => {}} />);
    expect(container.textContent).not.toContain("Claude Code");
    await click(button("installDialog.install"));
    expect(api.installSkill).toHaveBeenLastCalledWith(expect.objectContaining({ agents: ["codex", "co-owner"] }));
    await click(button("installDialog.selectAll"));
    await click(button("installDialog.install"));
    expect(api.installSkill).toHaveBeenLastCalledWith(expect.objectContaining({ agents: ["codex", "co-owner"] }));
  });

  it("excludes hidden share targets while retaining shared-directory co-owner checks", async () => {
    localStorage.setItem(HIDDEN_AGENTS_KEY, '["co-owner"]');
    await render(<ShareDialog open skill={skills[2]} scope={{ kind: "global" }} onClose={() => {}} onChanged={() => {}} />);
    expect(container.textContent).not.toContain("Co-owner");
    expect(button("Claude Code")!.textContent).toContain("shareDialog.coOwnerNote");
    await click(button("shareDialog.action"));
    expect(api.shareSkill).toHaveBeenLastCalledWith(expect.objectContaining({ agents: ["claude-code"] }));
    await click(button("installDialog.selectAll"));
    await click(button("shareDialog.action"));
    expect(api.shareSkill).toHaveBeenLastCalledWith(expect.objectContaining({ agents: ["claude-code"] }));
  });
});
