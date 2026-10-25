/**
 * Typed API layer over the new Tauri IPC surface (commands/vercel.rs).
 * Every backend call the GUI makes goes through this file; nothing else
 * imports `invoke` directly.
 */

import { invoke } from "@tauri-apps/api/core";

// ---------------------------------------------------------------------------
// DTOs (mirror the Rust structs; camelCase on the wire)
// ---------------------------------------------------------------------------

export type Scope =
  | { kind: "global" }
  | { kind: "project"; root: string };

export interface AgentExposure {
  /** Agent key when known, otherwise the CLI display name. */
  id: string;
  name: string;
  exposure: "shared" | "symlink" | "copy" | "native" | "unknown";
}

export type SkillStatus =
  | "installed"
  | "untracked"
  | "broken-link"
  | "metadata-unsupported";

export interface SkillView {
  id: string;
  slug: string;
  name: string;
  description: string | null;
  scope: "global" | "project";
  path: string;
  status: SkillStatus;
  source: string | null;
  sourceUrl: string | null;
  sourceType: string | null;
  installedAt: string | null;
  updatedAt: string | null;
  agents: AgentExposure[];
  isTrackedByVercel: boolean;
}

export interface CatalogResult {
  skills: SkillView[];
  diagnostics: string[];
  lockState: "ok" | "missing" | "unsupported" | "corrupt";
  cliAvailable: boolean;
}

export interface RuntimeStatus {
  nodePath: string | null;
  nodeVersion: string | null;
  cliPath: string | null;
  cliVersion: string | null;
  cliSource: string | null;
  writesSupported: boolean;
  degraded: boolean;
  globalSkillsDir: string;
  globalLockPath: string;
  notes: string[];
}

export interface AgentStatus {
  id: string;
  name: string;
  detected: boolean;
  supportsGlobal: boolean;
  globalDir: string | null;
  projectDir: string;
}

export interface ProjectInfo {
  root: string;
  name: string;
  hasLock: boolean;
  skillsDirExists: boolean;
}

export interface SkillDocument {
  markdown: string;
  truncated: boolean;
}

export interface MarketSkill {
  name: string;
  slug: string;
  source: string;
  installs: number;
}

export interface OperationItem {
  name: string;
  status: string;
  detail: string | null;
}

export interface OperationResult {
  success: boolean;
  exitCode: number | null;
  message: string;
  stdout: string | null;
  stderr: string | null;
  items: OperationItem[];
  cancelled: boolean;
  timedOut: boolean;
}

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

export function getRuntimeStatus(): Promise<RuntimeStatus> {
  return invoke("get_runtime_status");
}

export function setCliPath(path: string | null): Promise<RuntimeStatus> {
  return invoke("set_cli_path", { path });
}

export function getLastCliError(): Promise<string | null> {
  return invoke("get_last_cli_error");
}

export function listSkills(scope: Scope): Promise<CatalogResult> {
  return invoke("list_skills", { scopeArg: scope });
}

export function readSkillDoc(scope: Scope, path: string): Promise<SkillDocument> {
  return invoke("read_skill_doc", { scopeArg: scope, path });
}

export function listAgentStatus(scope: Scope): Promise<AgentStatus[]> {
  return invoke("list_agent_status", { scopeArg: scope });
}

export function searchMarketplace(
  query: string,
  owner: string | null,
): Promise<MarketSkill[]> {
  return invoke("search_marketplace", { query, owner });
}

export function openProject(path: string): Promise<ProjectInfo> {
  return invoke("open_project", { path });
}

export function closeProject(): Promise<void> {
  return invoke("close_project");
}

export function revealSkillFolder(scope: Scope, path: string): Promise<void> {
  return invoke("reveal_skill_folder", { scopeArg: scope, path });
}

export interface InstallRequest {
  source: string;
  scope: Scope;
  agents: string[];
  skills: string[] | null;
  copy: boolean;
  opId: string;
}

export function installSkill(request: InstallRequest): Promise<OperationResult> {
  return invoke("install_skill", { request });
}

export interface RemoveRequest {
  names: string[];
  scope: Scope;
  agents: string[] | null;
  opId: string;
}

export function removeSkill(request: RemoveRequest): Promise<OperationResult> {
  return invoke("remove_skill", { request });
}

export interface UpdateRequest {
  name: string;
  scope: Scope;
  opId: string;
}

export function updateSkill(request: UpdateRequest): Promise<OperationResult> {
  return invoke("update_skill", { request });
}

export interface ShareSkillRequest {
  /** Absolute path of the skill folder to link at. */
  path: string;
  scope: Scope;
  agents: string[];
  /** Replace real folders at agent dirs with links (user-confirmed). */
  replaceCopies: boolean;
}

export interface ShareItem {
  agent: string;
  /** linked | replaced | replaced-copy | skipped | failed | conflict
   *  (`conflict` = target holds user content a confirmed re-run replaces) */
  status: string;
  detail: string | null;
}

/** Self-managed sharing: symlink agent skills dirs at an installed skill. */
export function shareSkill(request: ShareSkillRequest): Promise<ShareItem[]> {
  return invoke("share_skill", { request });
}

export interface UnshareSkillRequest {
  /** Absolute path of the skill folder the links point at. */
  path: string;
  scope: Scope;
  agents: string[];
}

/** Self-managed unshare: remove agent links that point at the skill folder
 *  (`removed | skipped | failed` — real copies are skipped, never deleted). */
export function unshareSkill(request: UnshareSkillRequest): Promise<ShareItem[]> {
  return invoke("unshare_skill", { request });
}

export function cancelOperation(opId: string): Promise<boolean> {
  return invoke("cancel_operation", { opId });
}

export function appExit(): Promise<void> {
  return invoke("app_exit");
}

export function hideToTray(): Promise<void> {
  return invoke("hide_to_tray");
}

/** Event emitted by the backend watcher (and after write operations). */
export const SKILLS_CHANGED_EVENT = "skills-changed";

export function newOpId(): string {
  return `op-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}
