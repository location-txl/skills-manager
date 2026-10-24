import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Download, ListChecks, RefreshCw, Trash2 } from "lucide-react";
import { useApp } from "../context/AppContext";
import * as api from "../lib/api";
import type { SkillView } from "../lib/api";
import { SkillCard } from "../components/SkillCard";
import { SkillDetail } from "../components/SkillDetail";
import { InstallDialog } from "../components/InstallDialog";
import { SearchInput } from "../components/SearchInput";
import { MultiSelectToolbar } from "../components/MultiSelectToolbar";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { useMultiSelect } from "../hooks/useMultiSelect";
import { cn } from "../utils";

type StatusFilter = "all" | "installed" | "untracked";

/** Global skill library: everything under ~/.agents/skills plus every agent
 *  directory, deduplicated by the backend into one view. */
export function Library() {
  const { t } = useTranslation();
  const {
    runtime,
    globalCatalog,
    globalLoading,
    refreshGlobal,
    detailSkill,
    openSkillDetail,
    closeSkillDetail,
  } = useApp();
  const [query, setQuery] = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [agentFilter, setAgentFilter] = useState("");
  const [installOpen, setInstallOpen] = useState(false);
  const [batchRemove, setBatchRemove] = useState(false);

  const skills = useMemo(() => globalCatalog?.skills ?? [], [globalCatalog]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return skills.filter((skill) => {
      if (statusFilter === "installed" && !skill.isTrackedByVercel) return false;
      if (statusFilter === "untracked" && skill.isTrackedByVercel) return false;
      if (agentFilter && !skill.agents.some((a) => a.id === agentFilter)) return false;
      if (!q) return true;
      return (
        skill.name.toLowerCase().includes(q) ||
        skill.slug.includes(q) ||
        (skill.description ?? "").toLowerCase().includes(q) ||
        (skill.source ?? "").toLowerCase().includes(q)
      );
    });
  }, [skills, query, statusFilter, agentFilter]);

  const multi = useMultiSelect<SkillView>({
    items: skills,
    filtered,
    getKey: (s) => s.id,
    isItemActive: () => true,
    filterSignal: `${statusFilter}|${agentFilter}|${query}`,
  });

  const agentOptions = useMemo(() => {
    const ids = new Map<string, string>();
    for (const skill of skills) {
      for (const agent of skill.agents) ids.set(agent.id, agent.name);
    }
    return [...ids.entries()].sort((a, b) => a[1].localeCompare(b[1]));
  }, [skills]);

  const selectedSkills = useMemo(
    () => skills.filter((s) => multi.selectedIds.has(s.id)),
    [skills, multi.selectedIds],
  );

  const batchUninstall = async () => {
    const result = await api.removeSkill({
      names: selectedSkills.map((s) => s.slug),
      scope: { kind: "global" },
      agents: null,
      opId: api.newOpId(),
    });
    if (result.success) {
      toast.success(result.message);
      multi.exitMultiSelect();
    } else {
      toast.error(result.message);
    }
    refreshGlobal();
  };

  return (
    <div className="app-page">
      <div className="app-page-header flex items-start justify-between gap-3">
        <div>
          <h1 className="app-page-title">{t("library.title")}</h1>
          <p className="app-page-subtitle">
            {t("library.subtitle", { count: skills.length })}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            onClick={() => multi.setIsMultiSelect(!multi.isMultiSelect)}
            className={cn("app-toolbar-button app-toolbar-button-secondary")}
          >
            <ListChecks className="h-3.5 w-3.5" />
            {multi.isMultiSelect ? t("common.done") : t("common.select")}
          </button>
          <button
            onClick={refreshGlobal}
            disabled={globalLoading}
            className="app-toolbar-button app-toolbar-button-secondary"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", globalLoading && "animate-spin")} />
            {t("common.refresh")}
          </button>
          <button
            onClick={() => setInstallOpen(true)}
            disabled={runtime ? !runtime.writesSupported : true}
            className="app-toolbar-button app-toolbar-button-primary"
          >
            <Download className="h-3.5 w-3.5" />
            {t("library.install")}
          </button>
        </div>
      </div>

      {/* Toolbar */}
      <div className="app-toolbar">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("library.searchPlaceholder")}
          className="w-72"
        />
        <div className="flex flex-wrap items-center gap-2">
          <div className="app-segmented app-toolbar-segmented">
            {(["all", "installed", "untracked"] as const).map((key) => (
              <button
                key={key}
                onClick={() => setStatusFilter(key)}
                className={cn(
                  "app-segmented-button",
                  statusFilter === key && "app-segmented-button-active",
                )}
              >
                {t(`library.filter.${key}`)}
              </button>
            ))}
          </div>
          {agentOptions.length > 0 ? (
            <select
              value={agentFilter}
              onChange={(e) => setAgentFilter(e.target.value)}
              className="app-input h-10 w-44 text-[13px]"
            >
              <option value="">{t("library.allAgents")}</option>
              {agentOptions.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
          ) : null}
        </div>
      </div>

      {/* Lock state notice */}
      {globalCatalog &&
      globalCatalog.lockState !== "ok" &&
      globalCatalog.lockState !== "missing" ? (
        <div className="app-panel-muted p-3 text-[12px] leading-5 text-amber-700 dark:text-amber-400">
          {t("library.lockUnavailable")}
        </div>
      ) : null}
      {globalCatalog?.diagnostics.length ? (
        <details className="app-panel-muted px-4 py-3 text-[12px] text-tertiary">
          <summary className="cursor-pointer select-none outline-none">
            {t("library.diagnosticsCount", { count: globalCatalog.diagnostics.length })}
          </summary>
          <ul className="mt-2 space-y-1">
            {globalCatalog.diagnostics.map((d, i) => (
              <li key={i} className="leading-5">
                · {d}
              </li>
            ))}
          </ul>
        </details>
      ) : null}

      {/* Grid */}
      {globalLoading && skills.length === 0 ? (
        <div className="py-16 text-center text-[13px] text-muted">{t("common.loading")}</div>
      ) : filtered.length === 0 ? (
        <div className="app-panel-muted flex flex-col items-center gap-3 px-6 py-16 text-center">
          <p className="text-[13px] text-tertiary">{t("library.empty")}</p>
          <button onClick={() => setInstallOpen(true)} className="app-button-secondary">
            <Download className="h-3.5 w-3.5" />
            {t("library.findSkills")}
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {filtered.map((skill) => (
            <SkillCard
              key={skill.id}
              skill={skill}
              onOpen={openSkillDetail}
              selectionMode={multi.isMultiSelect}
              selected={multi.selectedIds.has(skill.id)}
              onToggleSelect={(s) => multi.toggleSelect(s.id)}
            />
          ))}
        </div>
      )}

      {/* Multi-select */}
      <MultiSelectToolbar
        selectedCount={multi.selectedIds.size}
        isAllSelected={multi.isAllSelected}
        actions={[
          {
            key: "uninstall",
            label: t("common.uninstall"),
            icon: <Trash2 className="h-3.5 w-3.5" />,
            tone: "danger",
            onSelect: () => setBatchRemove(true),
          },
        ]}
        labels={{
          hint: t("common.multiSelectHint"),
          selected: t("common.selectedCount", { count: multi.selectedIds.size }),
          selectAll: t("common.selectAll"),
          deselectAll: t("common.deselectAll"),
          cancel: t("common.cancel"),
          more: t("common.more"),
        }}
        onSelectAll={multi.handleSelectAll}
        onCancel={multi.exitMultiSelect}
      />

      {/* Detail sheet */}
      <SkillDetail
        skill={detailSkill}
        scope={{ kind: "global" }}
        onClose={closeSkillDetail}
        onChanged={refreshGlobal}
      />

      <InstallDialog
        open={installOpen}
        source=""
        onClose={() => setInstallOpen(false)}
        onInstalled={refreshGlobal}
      />

      <ConfirmDialog
        open={batchRemove}
        title={t("library.batchRemoveTitle")}
        message={t("library.batchRemoveMessage", { count: selectedSkills.length })}
        details={selectedSkills.map((s) => s.name)}
        confirmLabel={t("common.uninstall")}
        onClose={() => setBatchRemove(false)}
        onConfirm={batchUninstall}
      />
    </div>
  );
}
