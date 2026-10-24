import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Copy as CopyIcon, Download, X } from "lucide-react";
import type { OperationResult, Scope } from "../lib/api";
import * as api from "../lib/api";
import { useApp } from "../context/AppContext";
import { AgentIcon } from "./AgentIcon";
import { ToggleSwitch } from "./ToggleSwitch";
import { getErrorMessage } from "../lib/error";
import { cn } from "../utils";

interface Props {
  open: boolean;
  /** Pre-filled source (marketplace result); editable when free-form. */
  source: string;
  sourceLocked?: boolean;
  /** Skill names to pass via -s (null/omitted = let the CLI decide). */
  skills?: string[] | null;
  onClose: () => void;
  onInstalled: () => void;
}

/**
 * The install flow: explicit scope + agent selection, optional copy mode,
 * then one CLI call with a live progress state and per-skill results.
 * `--yes` is only sent because every option here was explicitly chosen.
 */
export function InstallDialog({
  open,
  source,
  sourceLocked = false,
  skills = null,
  onClose,
  onInstalled,
}: Props) {
  const { t } = useTranslation();
  const { runtime, project, agents } = useApp();
  const [sourceValue, setSourceValue] = useState(source);
  const [useProject, setUseProject] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [copy, setCopy] = useState(false);
  const [running, setRunning] = useState(false);
  const [opId, setOpId] = useState<string | null>(null);
  const [result, setResult] = useState<OperationResult | null>(null);

  useEffect(() => {
    if (!open) return;
    setSourceValue(source);
    setResult(null);
    setRunning(false);
    setOpId(null);
    // Default to the detected agents that are actually installed.
    setSelected(
      new Set(agents.filter((a) => a.detected && a.supportsGlobal).slice(0, 6).map((a) => a.id)),
    );
  }, [open, source, agents]);

  const scope: Scope = useMemo(
    () =>
      useProject && project
        ? { kind: "project", root: project.root }
        : { kind: "global" },
    [useProject, project],
  );

  const selectableAgents = useMemo(
    () =>
      agents.filter((a) =>
        useProject ? a.id !== "promptscript" : a.supportsGlobal,
      ),
    [agents, useProject],
  );

  if (!open) return null;

  const writesBlocked = runtime ? !runtime.writesSupported : true;

  const toggle = (id: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => {
    setSelected(new Set(selectableAgents.map((a) => a.id)));
  };

  const start = async () => {
    if (selected.size === 0) {
      toast.error(t("installDialog.selectAgents"));
      return;
    }
    setRunning(true);
    const id = api.newOpId();
    setOpId(id);
    try {
      const res = await api.installSkill({
        source: sourceValue.trim(),
        scope,
        agents: [...selected],
        skills,
        copy,
        opId: id,
      });
      setResult(res);
      if (res.success) {
        toast.success(res.message);
        onInstalled();
      } else {
        toast.error(res.message);
      }
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    } finally {
      setRunning(false);
      setOpId(null);
    }
  };

  const cancel = async () => {
    if (opId) await api.cancelOperation(opId);
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
            <Download className="h-4 w-4 text-accent" />
            {t("installDialog.title")}
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
          {/* Source */}
          <label className="block">
            <span className="mb-1.5 block text-[12px] font-medium text-tertiary">
              {t("installDialog.source")}
            </span>
            <input
              type="text"
              value={sourceValue}
              disabled={sourceLocked || running}
              onChange={(e) => setSourceValue(e.target.value)}
              placeholder="owner/repo · https://… · /local/path"
              className="app-input w-full disabled:opacity-60"
            />
          </label>

          {/* Scope */}
          <div>
            <span className="mb-1.5 block text-[12px] font-medium text-tertiary">
              {t("installDialog.scope")}
            </span>
            <div className="app-segmented w-full">
              <button
                onClick={() => setUseProject(false)}
                disabled={running}
                className={cn(
                  "app-segmented-button flex-1",
                  !useProject && "app-segmented-button-active",
                )}
              >
                {t("installDialog.global")}
              </button>
              <button
                onClick={() => setUseProject(true)}
                disabled={running || !project}
                title={project ? project.root : t("installDialog.noProject")}
                className={cn(
                  "app-segmented-button flex-1 disabled:opacity-50",
                  useProject && "app-segmented-button-active",
                )}
              >
                {t("installDialog.project")}
              </button>
            </div>
            {useProject && project ? (
              <div className="mt-1.5 truncate text-[11px] text-muted">{project.root}</div>
            ) : null}
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
              {selectableAgents.map((agent) => (
                <button
                  key={agent.id}
                  onClick={() => toggle(agent.id)}
                  disabled={running}
                  className={cn(
                    "flex items-center gap-2 rounded-md border px-2 py-1.5 text-left text-[12px] transition-colors outline-none",
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
                  {!agent.detected ? (
                    <span className="shrink-0 text-[11px] text-faint">
                      {t("installDialog.notInstalled")}
                    </span>
                  ) : null}
                </button>
              ))}
            </div>
            <p className="mt-1.5 text-[11px] leading-4 text-faint">
              {t("installDialog.agentsHint")}
            </p>
          </div>

          {/* Copy mode */}
          <div className="flex items-center justify-between rounded-lg border border-border-subtle bg-bg-secondary px-3 py-2.5">
            <div>
              <div className="text-[12px] font-medium text-secondary">
                {t("installDialog.copyMode")}
              </div>
              <div className="text-[11px] text-muted">{t("installDialog.copyModeHint")}</div>
            </div>
            <ToggleSwitch
              checked={copy}
              onChange={() => setCopy((v) => !v)}
              disabled={running}
            />
          </div>

          {skills && skills.length > 0 ? (
            <div className="rounded-lg border border-border-subtle bg-bg-secondary px-3 py-2.5 text-[12px] text-tertiary">
              {t("installDialog.skillsFilter")}: {skills.join(", ")}
            </div>
          ) : (
            <p className="text-[11px] leading-4 text-faint">
              {t("installDialog.allSkillsHint")}
            </p>
          )}

          {/* Result details */}
          {result ? (
            <div
              className={cn(
                "rounded-lg border p-3 text-[12px]",
                result.success
                  ? "border-emerald-500/30 bg-emerald-500/5 text-emerald-700 dark:text-emerald-400"
                  : "border-amber-500/30 bg-amber-500/5 text-amber-700 dark:text-amber-400",
              )}
            >
              <div className="font-medium">{result.message}</div>
              {result.items.length > 0 ? (
                <ul className="mt-1.5 space-y-1">
                  {result.items.map((item) => (
                    <li key={item.name} className="flex items-baseline gap-2">
                      <span className="font-mono text-[11px]">{item.name}</span>
                      <span className="text-muted">{item.status}</span>
                      {item.detail ? (
                        <span className="min-w-0 flex-1 truncate text-faint">
                          {item.detail}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              ) : null}
            </div>
          ) : null}
        </div>

        {/* Footer */}
        <div className="mt-4 flex items-center justify-end gap-2">
          {running ? (
            <>
              <span className="mr-auto text-[12px] text-muted">
                {t("installDialog.running")}
              </span>
              <button onClick={cancel} className="app-button-secondary">
                <X className="h-3.5 w-3.5" />
                {t("common.cancel")}
              </button>
            </>
          ) : (
            <>
              {writesBlocked ? (
                <span className="mr-auto max-w-[55%] text-[11px] leading-4 text-amber-600 dark:text-amber-400">
                  {t("installDialog.writesBlocked")}
                </span>
              ) : null}
              <button onClick={onClose} className="app-button-secondary">
                {result ? t("common.close") : t("common.cancel")}
              </button>
              <button
                onClick={start}
                disabled={running || writesBlocked || !sourceValue.trim()}
                className="app-button-primary"
              >
                <CopyIcon className="h-3.5 w-3.5" />
                {t("installDialog.install")}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
