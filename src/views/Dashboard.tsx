import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import {
  CircleCheck,
  CircleAlert,
  Download,
  Layers,
  Bot,
  FolderOpen,
  RefreshCw,
} from "lucide-react";
import { useApp } from "../context/AppContext";
import { cn } from "../utils";

/** Overview: runtime health, what's installed, where to go next. */
export function Dashboard() {
  const { t } = useTranslation();
  const {
    runtime,
    runtimeLoading,
    globalCatalog,
    agents,
    project,
    projectCatalog,
    refreshAll,
    refreshing,
  } = useApp();

  const skills = globalCatalog?.skills ?? [];
  const tracked = skills.filter((s) => s.isTrackedByVercel).length;
  const detectedAgents = agents.filter((a) => a.detected);
  const healthy = runtime !== null && !runtime.degraded;

  return (
    <div className="app-page">
      <div className="app-page-header flex items-start justify-between">
        <div>
          <h1 className="app-page-title">{t("dashboard.title")}</h1>
          <p className="app-page-subtitle">{t("dashboard.subtitle")}</p>
        </div>
        <button
          onClick={refreshAll}
          disabled={refreshing}
          className="app-toolbar-button app-toolbar-button-secondary"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", refreshing && "animate-spin")} />
          {t("common.refresh")}
        </button>
      </div>

      {/* Runtime health */}
      <Link
        to="/settings"
        className="app-panel flex items-center gap-3 p-4 transition-colors hover:bg-surface-hover"
      >
        {runtimeLoading ? (
          <CircleAlert className="h-5 w-5 shrink-0 text-muted" />
        ) : healthy ? (
          <CircleCheck className="h-5 w-5 shrink-0 text-emerald-500" />
        ) : (
          <CircleAlert className="h-5 w-5 shrink-0 text-amber-500" />
        )}
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-medium text-primary">
            {runtimeLoading
              ? t("dashboard.checkingRuntime")
              : healthy
                ? t("dashboard.runtimeOk", { version: runtime?.cliVersion ?? "?" })
                : t("dashboard.runtimeDegraded")}
          </div>
          <div className="mt-0.5 truncate text-[12px] text-muted">
            {t("dashboard.runtimeDetail", {
              node: runtime?.nodeVersion ?? "—",
              cli: runtime?.cliVersion ?? "—",
            })}
          </div>
        </div>
      </Link>

      {/* Stats */}
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        <StatCard
          icon={<Layers className="h-4 w-4" />}
          label={t("dashboard.globalSkills")}
          value={skills.length}
          sub={t("dashboard.trackedCount", { count: tracked })}
          to="/library"
        />
        <StatCard
          icon={<Bot className="h-4 w-4" />}
          label={t("dashboard.detectedAgents")}
          value={detectedAgents.length}
          sub={t("dashboard.agentsSub")}
          to="/agents"
        />
        <StatCard
          icon={<FolderOpen className="h-4 w-4" />}
          label={t("dashboard.project")}
          value={project ? (projectCatalog?.skills.length ?? 0) : "—"}
          sub={project ? project.name : t("dashboard.noProject")}
          to="/project"
        />
        <StatCard
          icon={<Download className="h-4 w-4" />}
          label={t("dashboard.marketplace")}
          value="·"
          sub={t("dashboard.marketplaceSub")}
          to="/marketplace"
        />
      </div>

      {/* Diagnostics surfaced by the catalog */}
      {globalCatalog && globalCatalog.diagnostics.length > 0 ? (
        <div className="app-panel-muted p-4">
          <div className="app-section-title mb-2">{t("dashboard.diagnostics")}</div>
          <ul className="space-y-1.5">
            {globalCatalog.diagnostics.slice(0, 5).map((d, i) => (
              <li key={i} className="text-[12px] leading-5 text-tertiary">
                · {d}
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}

function StatCard({
  icon,
  label,
  value,
  sub,
  to,
}: {
  icon: React.ReactNode;
  label: string;
  value: number | string;
  sub: string;
  to: string;
}) {
  return (
    <Link
      to={to}
      className="app-panel flex flex-col gap-2 p-4 transition-colors hover:bg-surface-hover"
    >
      <div className="flex items-center gap-2 text-muted">
        {icon}
        <span className="text-[12px] font-medium">{label}</span>
      </div>
      <div className="text-[17px] font-semibold leading-7 text-primary">{value}</div>
      <div className="truncate text-[12px] text-muted">{sub}</div>
    </Link>
  );
}
