import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { Download, FolderOpen, RefreshCw, X } from "lucide-react";
import { useApp } from "../context/AppContext";
import * as api from "../lib/api";
import type { SkillView } from "../lib/api";
import { SkillCard } from "../components/SkillCard";
import { SkillDetail } from "../components/SkillDetail";
import { InstallDialog } from "../components/InstallDialog";
import { SearchInput } from "../components/SearchInput";
import { compactHomePath, cn } from "../utils";

/** Project skills: one folder picked per session, read from
 *  `<project>/.agents/skills` + `skills-lock.json`. No project registry. */
export function ProjectView() {
  const { t } = useTranslation();
  const {
    runtime,
    project,
    projectCatalog,
    projectLoading,
    openProject,
    closeProject,
    refreshProject,
  } = useApp();
  const [query, setQuery] = useState("");
  const [installOpen, setInstallOpen] = useState(false);
  const [detail, setDetail] = useState<SkillView | null>(null);

  const pickFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    const info = await openProject(selected);
    if (info) toast.success(t("project.opened", { name: info.name }));
  };

  const skills = projectCatalog?.skills ?? [];
  const filtered = skills.filter((s) => {
    const q = query.trim().toLowerCase();
    if (!q) return true;
    return (
      s.name.toLowerCase().includes(q) ||
      s.slug.includes(q) ||
      (s.description ?? "").toLowerCase().includes(q)
    );
  });

  if (!project) {
    return (
      <div className="app-page app-page-narrow">
        <div className="app-page-header">
          <h1 className="app-page-title">{t("project.title")}</h1>
          <p className="app-page-subtitle">{t("project.subtitle")}</p>
        </div>
        <div className="app-panel flex flex-col items-center gap-4 px-6 py-16 text-center">
          <FolderOpen className="h-8 w-8 text-muted" />
          <p className="max-w-md text-[13px] leading-6 text-tertiary">
            {t("project.pickerHint")}
          </p>
          <button onClick={pickFolder} className="app-button-primary">
            <FolderOpen className="h-4 w-4" />
            {t("project.chooseFolder")}
          </button>
        </div>
      </div>
    );
  }

  const scope: api.Scope = { kind: "project", root: project.root };

  return (
    <div className="app-page">
      <div className="app-page-header flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h1 className="app-page-title">{project.name}</h1>
          <p className="app-page-subtitle truncate font-mono text-[12px]">
            {project.root}
          </p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <button
            onClick={refreshProject}
            disabled={projectLoading}
            className="app-toolbar-button app-toolbar-button-secondary"
          >
            <RefreshCw className={cn("h-3.5 w-3.5", projectLoading && "animate-spin")} />
            {t("common.refresh")}
          </button>
          <button
            onClick={() => {
              closeProject();
              setDetail(null);
            }}
            className="app-toolbar-button app-toolbar-button-secondary"
          >
            <X className="h-3.5 w-3.5" />
            {t("project.close")}
          </button>
          <button
            onClick={() => setInstallOpen(true)}
            disabled={runtime ? !runtime.writesSupported : true}
            className="app-toolbar-button app-toolbar-button-primary"
          >
            <Download className="h-3.5 w-3.5" />
            {t("project.installHere")}
          </button>
        </div>
      </div>

      {/* Project facts */}
      <div className="flex flex-wrap items-center gap-2">
        <span className="app-badge">
          {t("project.skillCount", { count: skills.length })}
        </span>
        <span className="app-badge">{project.hasLock ? t("project.hasLock") : t("project.noLock")}</span>
        <span className="app-badge">{compactHomePath(`${project.root}/.agents/skills`)}</span>
      </div>

      <div className="app-toolbar">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("library.searchPlaceholder")}
          className="w-72"
        />
        <button onClick={pickFolder} className="app-toolbar-button app-toolbar-button-secondary">
          <FolderOpen className="h-3.5 w-3.5" />
          {t("project.switchFolder")}
        </button>
      </div>

      {projectCatalog?.diagnostics.length ? (
        <details className="app-panel-muted px-4 py-3 text-[12px] text-tertiary">
          <summary className="cursor-pointer select-none outline-none">
            {t("library.diagnosticsCount", { count: projectCatalog.diagnostics.length })}
          </summary>
          <ul className="mt-2 space-y-1">
            {projectCatalog.diagnostics.map((d, i) => (
              <li key={i} className="leading-5">
                · {d}
              </li>
            ))}
          </ul>
        </details>
      ) : null}

      {filtered.length === 0 ? (
        <div className="app-panel-muted px-6 py-14 text-center text-[13px] text-tertiary">
          {t("project.empty")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
          {filtered.map((skill) => (
            <SkillCard key={skill.id} skill={skill} onOpen={setDetail} />
          ))}
        </div>
      )}

      <SkillDetail
        skill={skills.find((skill) => skill.id === detail?.id) ?? null}
        scope={scope}
        onClose={() => setDetail(null)}
        onChanged={refreshProject}
      />

      <InstallDialog
        open={installOpen}
        source=""
        onClose={() => setInstallOpen(false)}
        onInstalled={refreshProject}
      />
    </div>
  );
}
