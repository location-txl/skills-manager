import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Lock } from "lucide-react";
import type { AgentStatus, Scope, ShareItem, SkillView } from "../lib/api";
import * as api from "../lib/api";
import { exposureOf, isSharedDirAgent } from "../lib/shareRules";
import { useApp } from "../context/AppContext";
import { AgentIcon } from "./AgentIcon";
import { getErrorMessage } from "../lib/error";
import { cn } from "../utils";

interface Props {
  skill: SkillView;
  scope: Scope;
}

interface Row {
  agent: AgentStatus;
  exposure: ReturnType<typeof exposureOf>;
  locked: boolean;
}

/** Hover strip on a skill card: one icon per relevant agent (detected, or
 *  already seeing the skill) — colored when it has the skill, gray when not,
 *  lock-badged when it reads the shared dir directly. One click shares the
 *  skill to that agent or removes its link; the "all" pill shares to every
 *  gray unlocked agent at once. */
export function AgentShareBar({ skill, scope }: Props) {
  const { t } = useTranslation();
  const { agents, runtime, refreshAll } = useApp();
  const [running, setRunning] = useState(false);

  const globalSkillsDir = runtime?.globalSkillsDir ?? null;
  const rows = useMemo<Row[]>(
    () =>
      agents
        .filter((a) => (scope.kind === "global" ? a.supportsGlobal && a.globalDir : true))
        .map((agent) => ({
          agent,
          exposure: exposureOf(skill, agent),
          locked: isSharedDirAgent(agent, scope, globalSkillsDir),
        }))
        // 79 icons cannot fit a card row: show the agents the user actually
        // has (detected) plus anyone already seeing the skill. Name order is
        // fixed — sharing only flips an icon's color in place, never moves it.
        .filter((r) => r.agent.detected || r.exposure)
        .sort((x, y) => x.agent.name.localeCompare(y.agent.name)),
    [agents, skill, scope, globalSkillsDir],
  );

  const shareAllTargets = rows.filter((r) => !r.exposure && !r.locked);

  /** Run the op, toast the per-agent outcome, refresh. No optimistic update
   *  and no confirmation flow: the backend never destroys user content here
   *  (`replaceCopies: false`), and anything it refuses is pointed at the
   *  detail page, where ShareDialog keeps its retry confirmation. */
  const apply = async (op: Promise<ShareItem[]>, multi: boolean) => {
    setRunning(true);
    try {
      const items = await op;
      if (multi) {
        const bad = items.filter((i) => i.status === "conflict" || i.status === "failed");
        if (bad.length > 0) {
          toast.error(t("shareDialog.doneWithFailures", { count: bad.length }), {
            description: bad
              .map((i) => `${i.agent}${i.detail ? ` — ${i.detail}` : ""}`)
              .join("\n"),
          });
        } else {
          toast.success(t("agentShareBar.shareAllDone", { count: items.length }));
        }
      } else {
        for (const item of items) {
          if (item.status === "failed") {
            toast.error(t("agentShareBar.failedToast", { name: item.agent }), {
              description: item.detail ?? undefined,
            });
          } else if (item.status === "conflict") {
            toast.warning(t("agentShareBar.conflictHint", { name: item.agent }), {
              description: item.detail ?? undefined,
            });
          } else if (item.status === "skipped") {
            toast.info(t("agentShareBar.skippedToast", { name: item.agent }), {
              description: item.detail ?? undefined,
            });
          } else if (item.status === "removed") {
            toast.success(t("agentShareBar.unsharedFrom", { name: item.agent }));
          } else {
            toast.success(t("agentShareBar.sharedWith", { name: item.agent }));
          }
        }
      }
      await refreshAll();
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    } finally {
      setRunning(false);
    }
  };

  const onIcon = (row: Row) => {
    if (row.locked) {
      toast.info(t("agentShareBar.lockedHint", { name: row.agent.name }));
      return;
    }
    if (!row.exposure) {
      void apply(
        api.shareSkill({
          path: skill.path,
          scope,
          agents: [row.agent.id],
          replaceCopies: false,
        }),
        false,
      );
      return;
    }
    if (row.exposure === "symlink") {
      void apply(api.unshareSkill({ path: skill.path, scope, agents: [row.agent.id] }), false);
      return;
    }
    // copy / native / unknown: the agent holds real content of its own; the
    // replace-with-link decision belongs to the detail page's confirm flow.
    toast.info(
      t("agentShareBar.managedHint", {
        name: row.agent.name,
        mode: t(`exposure.${row.exposure}`),
      }),
    );
  };

  const onShareAll = () => {
    if (shareAllTargets.length === 0) return;
    void apply(
      api.shareSkill({
        path: skill.path,
        scope,
        agents: shareAllTargets.map((r) => r.agent.id),
        replaceCopies: false,
      }),
      true,
    );
  };

  if (rows.length === 0) {
    return <span className="text-[12px] text-muted">{t("agents.notLinked")}</span>;
  }

  return (
    <div
      className="flex min-w-0 items-center gap-1"
      // The card root opens the detail on click; the strip handles its own.
      onClick={(e) => e.stopPropagation()}
      onKeyDown={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        onClick={onShareAll}
        disabled={running || shareAllTargets.length === 0}
        title={t("agentShareBar.shareAllTitle")}
        className="shrink-0 rounded-full border border-border-subtle bg-bg-secondary px-2 py-0.5 text-[11px] text-tertiary outline-none transition-colors hover:bg-surface-hover hover:text-secondary disabled:opacity-50"
      >
        {t("agentShareBar.all")}
      </button>
      {rows.map((row) => (
        <button
          key={row.agent.id}
          type="button"
          onClick={() => onIcon(row)}
          disabled={running}
          title={iconTitle(row, t)}
          className={cn(
            "relative h-5 w-5 shrink-0 rounded-md outline-none transition-[filter,opacity] disabled:opacity-50",
            row.locked ? "cursor-help" : "cursor-pointer",
            row.exposure
              ? "hover:opacity-80"
              : "opacity-40 grayscale hover:opacity-70",
          )}
        >
          <AgentIcon agentKey={row.agent.id} displayName={row.agent.name} className="h-5 w-5" />
          {row.locked ? (
            <span className="absolute -bottom-1 -right-1 flex h-3 w-3 items-center justify-center rounded-full border border-border-subtle bg-surface">
              <Lock className="h-2 w-2 text-faint" />
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

function iconTitle(row: Row, t: (k: string) => string): string {
  const state = row.exposure ? t(`exposure.${row.exposure}`) : t("agentShareBar.notShared");
  return row.locked
    ? `${row.agent.name} · ${state} · ${t("shareDialog.universalNote")}`
    : `${row.agent.name} · ${state}`;
}
