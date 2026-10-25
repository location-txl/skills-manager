import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Share2, X } from "lucide-react";
import type { AgentStatus, Scope, ShareItem, SkillView } from "../lib/api";
import * as api from "../lib/api";
import { useApp } from "../context/AppContext";
import { AgentIcon } from "./AgentIcon";
import { ConfirmDialog } from "./ConfirmDialog";
import { getErrorMessage } from "../lib/error";
import { cn } from "../utils";

interface Props {
  open: boolean;
  skill: SkillView;
  scope: Scope;
  onClose: () => void;
  /** Called after sharing so the caller can refresh the catalog. */
  onChanged: () => void;
}

/** Exposures where the agent dir holds real content a confirmed share replaces. */
const CONFLICTING_EXPOSURES = new Set(["copy", "native", "unknown"]);

const STATUS_KEY: Record<string, string> = {
  linked: "shareDialog.statusLinked",
  replaced: "shareDialog.statusReplaced",
  "replaced-copy": "shareDialog.statusReplacedCopy",
  skipped: "shareDialog.statusSkipped",
  failed: "shareDialog.statusFailed",
  conflict: "shareDialog.statusConflict",
};

/**
 * Share an installed skill: the app itself symlinks each selected agent's
 * skills dir at the skill's existing folder (core/symlink_share.rs). No
 * `skills add` round-trip, so no canonical rebuild and no copy fallback —
 * and it works even when the CLI is unavailable.
 */
export function ShareDialog({ open, skill, scope, onClose, onChanged }: Props) {
  const { t } = useTranslation();
  const { runtime, agents } = useApp();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<ShareItem[] | null>(null);
  const [conflictAgents, setConflictAgents] = useState<string[] | null>(null);
  /** Post-run conflicts (targets the catalog could not see) offered a retry. */
  const [retryAgents, setRetryAgents] = useState<string[] | null>(null);

  /** How a listed agent already sees the skill, if at all. */
  const exposureOf = (agent: AgentStatus) =>
    skill.agents.find((e) => e.id === agent.id || e.name === agent.name)
      ?.exposure;

  /** Agents whose skills dir IS the shared dir read it directly; a link
   *  there would expose the skill to every universal agent at once. */
  const isSharedDirAgent = (agent: AgentStatus) =>
    scope.kind === "global"
      ? !!agent.globalDir && agent.globalDir === runtime?.globalSkillsDir
      : agent.projectDir === ".agents/skills";

  /** Other agents reading this exact same dir (zencoder/zenflow share
   *  ~/.zencoder/skills; amp/replit/universal share ~/.config/agents/skills):
   *  the link is visible to them too, so say so instead of leaking silently. */
  const hasCoOwners = (agent: AgentStatus) =>
    agents.some(
      (a) =>
        a.id !== agent.id &&
        (scope.kind === "global"
          ? !!a.globalDir && a.globalDir === agent.globalDir
          : a.projectDir === agent.projectDir),
    );

  const selectableAgents = useMemo(
    () =>
      agents.filter((a) =>
        scope.kind === "global" ? a.supportsGlobal && a.globalDir : true,
      ),
    [agents, scope],
  );

  useEffect(() => {
    if (!open) return;
    setResult(null);
    setConflictAgents(null);
    setRetryAgents(null);
    // Detected agents that do not already see the skill; existing ones stay
    // off so a default confirm can never destroy anything.
    setSelected(
      new Set(
        selectableAgents
          .filter((a) => a.detected && !isSharedDirAgent(a) && !exposureOf(a))
          .map((a) => a.id),
      ),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, skill.path, selectableAgents]);

  if (!open) return null;

  const toggle = (id: string) => {
    setResult(null);
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    setResult(null);
    setSelected(
      new Set(selectableAgents.filter((a) => !isSharedDirAgent(a)).map((a) => a.id)),
    );
  };

  const run = async (replaceCopies: boolean) => {
    setRunning(true);
    try {
      const items = await api.shareSkill({
        path: skill.path,
        scope,
        agents: [...selected],
        replaceCopies,
      });
      setResult(items);
      const failed = items.filter((i) => i.status === "failed").length;
      // Conflicts the exposure pre-check cannot see (a link to elsewhere or
      // a dangling one carries no exposure for this skill) come back from
      // the backend — this retry confirmation is the only other route to a
      // confirmed re-run, so the dead-end stays closed.
      const conflicts = replaceCopies
        ? []
        : items.filter((i) => i.status === "conflict").map((i) => i.agent);
      if (conflicts.length > 0) {
        setRetryAgents(conflicts);
      } else if (failed === 0) {
        toast.success(t("shareDialog.done"));
      } else {
        toast.error(t("shareDialog.doneWithFailures", { count: failed }));
      }
      onChanged();
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    } finally {
      setRunning(false);
    }
  };

  const start = () => {
    if (selected.size === 0) {
      toast.error(t("installDialog.selectAgents"));
      return;
    }
    // The backend refuses unconfirmed conflicts too — this is the GUI half.
    const conflicts = selectableAgents
      .filter(
        (a) =>
          selected.has(a.id) &&
          CONFLICTING_EXPOSURES.has(exposureOf(a) ?? ""),
      )
      .map((a) => a.name);
    if (conflicts.length > 0) {
      setConflictAgents(conflicts);
      return;
    }
    void run(false);
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div
        className="absolute inset-0 bg-black/70 backdrop-blur-sm"
        onClick={() => {
          if (!running) onClose();
        }}
      />
      <div className="relative flex max-h-[calc(85vh/var(--app-scale))] w-full max-w-lg flex-col rounded-xl border border-border bg-surface p-5 shadow-2xl">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-[13px] font-semibold text-primary">
            <Share2 className="h-4 w-4 text-accent" />
            {t("shareDialog.title")}
          </h2>
          <button
            onClick={() => {
              if (!running) onClose();
            }}
            disabled={running}
            className="rounded-md p-1 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary disabled:opacity-50"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1">
          {/* Source folder (read-only) */}
          <div>
            <span className="mb-1.5 block text-[12px] font-medium text-tertiary">
              {t("shareDialog.source")}
            </span>
            <div
              title={skill.path}
              className="truncate rounded-lg border border-border-subtle bg-bg-secondary px-3 py-2 font-mono text-[11px] text-tertiary"
            >
              {skill.path}
            </div>
            <p className="mt-1.5 text-[11px] leading-4 text-faint">
              {t("shareDialog.hint")}
            </p>
          </div>

          {/* Agents */}
          <div>
            <div className="mb-1.5 flex items-center justify-between">
              <span className="text-[12px] font-medium text-tertiary">
                {t("installDialog.agents")}
              </span>
              <button
                onClick={selectAll}
                disabled={running}
                className="text-[11px] text-accent outline-none hover:underline disabled:opacity-50"
              >
                {t("installDialog.selectAll")}
              </button>
            </div>
            <div className="grid max-h-44 grid-cols-2 gap-1.5 overflow-y-auto rounded-lg border border-border-subtle bg-bg-secondary p-2">
              {selectableAgents.map((agent) => {
                const existingExposure = exposureOf(agent);
                const sharedDir = isSharedDirAgent(agent);
                return (
                  <button
                    key={agent.id}
                    onClick={() => toggle(agent.id)}
                    disabled={running || sharedDir}
                    title={sharedDir ? t("shareDialog.universalNote") : undefined}
                    className={cn(
                      "flex items-center gap-2 rounded-md border px-2 py-1.5 text-left text-[12px] transition-colors outline-none disabled:cursor-not-allowed disabled:opacity-50",
                      selected.has(agent.id)
                        ? "border-accent-border bg-surface-active text-primary"
                        : "border-transparent text-tertiary hover:bg-surface-hover",
                    )}
                  >
                    <AgentIcon
                      agentKey={agent.id}
                      displayName={agent.name}
                      className="h-4 w-4"
                    />
                    <span className="min-w-0 flex-1 truncate">{agent.name}</span>
                    {sharedDir ? (
                      <span className="shrink-0 text-[11px] text-faint">
                        {t("shareDialog.universalNote")}
                      </span>
                    ) : hasCoOwners(agent) ? (
                      <span className="shrink-0 text-[11px] text-faint">
                        {t("shareDialog.coOwnerNote")}
                      </span>
                    ) : existingExposure ? (
                      <span className="shrink-0 text-[11px] text-faint">
                        {t(`exposure.${existingExposure}`)}
                      </span>
                    ) : !agent.detected ? (
                      <span className="shrink-0 text-[11px] text-faint">
                        {t("installDialog.notInstalled")}
                      </span>
                    ) : null}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Result details */}
          {result ? (
            <div className="rounded-lg border border-border-subtle bg-bg-secondary p-3 text-[12px] text-tertiary">
              <ul className="space-y-1">
                {result.map((item) => (
                  <li key={item.agent} className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate">{item.agent}</span>
                    <span
                      className={cn(
                        "shrink-0",
                        item.status === "failed" ? "text-amber-600 dark:text-amber-400" : "text-muted",
                      )}
                    >
                      {t(STATUS_KEY[item.status] ?? "shareDialog.statusFailed")}
                    </span>
                    {item.detail && (item.status === "failed" || item.status === "conflict") ? (
                      <span className="min-w-0 flex-1 truncate text-faint" title={item.detail}>
                        {item.detail}
                      </span>
                    ) : null}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>

        {/* Footer */}
        <div className="mt-4 flex items-center justify-end gap-2">
          <button onClick={onClose} className="app-button-secondary" disabled={running}>
            {result ? t("common.close") : t("common.cancel")}
          </button>
          <button
            onClick={start}
            disabled={running || selected.size === 0}
            className="app-button-primary"
          >
            <Share2 className="h-3.5 w-3.5" />
            {t("shareDialog.action")}
          </button>
        </div>
      </div>

      <ConfirmDialog
        open={conflictAgents !== null}
        tone="warning"
        title={t("shareDialog.conflictTitle")}
        message={t("shareDialog.conflictMessage")}
        details={conflictAgents ?? []}
        confirmLabel={t("shareDialog.action")}
        onClose={() => setConflictAgents(null)}
        onConfirm={async () => {
          setConflictAgents(null);
          await run(true);
        }}
      />

      <ConfirmDialog
        open={retryAgents !== null}
        tone="warning"
        title={t("shareDialog.retryTitle")}
        message={t("shareDialog.retryMessage")}
        details={retryAgents ?? []}
        confirmLabel={t("shareDialog.action")}
        onClose={() => setRetryAgents(null)}
        onConfirm={async () => {
          setRetryAgents(null);
          await run(true);
        }}
      />
    </div>
  );
}
