import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, Info } from "lucide-react";
import { useApp } from "../context/AppContext";
import type { SkillView } from "../lib/api";
import { AgentIcon } from "../components/AgentIcon";
import { SkillCard } from "../components/SkillCard";
import { SkillDetail } from "../components/SkillDetail";
import { SearchInput } from "../components/SearchInput";
import { compactHomePath } from "../utils";

/** Agent workspaces: which agents exist, and what each can see. Visibility
 *  comes from the catalog (CLI `ls` + disk); this page never writes. */
export function AgentsView() {
  const { t } = useTranslation();
  const { agents, globalCatalog, detailSkill, openSkillDetail, closeSkillDetail, refreshGlobal } =
    useApp();
  const [selectedAgent, setSelectedAgent] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  const skills = useMemo(() => globalCatalog?.skills ?? [], [globalCatalog]);

  const perAgent = useMemo(() => {
    const map = new Map<string, SkillView[]>();
    for (const skill of skills) {
      for (const agent of skill.agents) {
        const list = map.get(agent.id) ?? [];
        list.push(skill);
        map.set(agent.id, list);
      }
    }
    return map;
  }, [skills]);

  const detected = agents.filter((a) => a.detected);
  const agent = selectedAgent ? agents.find((a) => a.id === selectedAgent) : null;

  const agentSkills = useMemo(() => {
    if (!agent) return [];
    const q = query.trim().toLowerCase();
    return (perAgent.get(agent.id) ?? []).filter(
      (s) => !q || s.name.toLowerCase().includes(q) || s.slug.includes(q),
    );
  }, [agent, perAgent, query]);

  if (agent) {
    return (
      <div className="app-page">
        <div className="app-page-header flex items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <button
              onClick={() => {
                setSelectedAgent(null);
                setQuery("");
              }}
              className="mt-1 rounded-md p-1.5 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary"
            >
              <ArrowLeft className="h-4 w-4" />
            </button>
            <div>
              <h1 className="app-page-title flex items-center gap-2.5">
                <AgentIcon agentKey={agent.id} displayName={agent.name} className="h-6 w-6" />
                {agent.name}
              </h1>
              <p className="app-page-subtitle font-mono text-[12px]">
                {compactHomePath(agent.globalDir ?? agent.projectDir)}
              </p>
            </div>
          </div>
          <SearchInput
            value={query}
            onChange={setQuery}
            className="w-64"
          />
        </div>

        <div className="app-panel-muted flex items-start gap-2.5 p-3 text-[12px] leading-5 text-tertiary">
          <Info className="mt-0.5 h-3.5 w-3.5 shrink-0 text-sky-500" />
          {t("agents.sharedVisibilityNote")}
        </div>

        {agentSkills.length === 0 ? (
          <div className="app-panel-muted px-6 py-14 text-center text-[13px] text-tertiary">
            {t("agents.noSkills", { name: agent.name })}
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {agentSkills.map((skill) => (
              <SkillCard key={`${agent.id}-${skill.id}`} skill={skill} onOpen={openSkillDetail} />
            ))}
          </div>
        )}

        <SkillDetail
          skill={detailSkill}
          scope={{ kind: "global" }}
          onClose={closeSkillDetail}
          onChanged={refreshGlobal}
        />
      </div>
    );
  }

  return (
    <div className="app-page">
      <div className="app-page-header">
        <h1 className="app-page-title">{t("agents.title")}</h1>
        <p className="app-page-subtitle">
          {t("agents.subtitle", {
            detected: detected.length,
            total: agents.length,
          })}
        </p>
      </div>

      <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
        {detected.map((a) => (
          <button
            key={a.id}
            onClick={() => setSelectedAgent(a.id)}
            className="app-panel flex items-center gap-3 p-4 text-left transition-colors outline-none hover:bg-surface-hover"
          >
            <AgentIcon agentKey={a.id} displayName={a.name} className="h-9 w-9" />
            <div className="min-w-0 flex-1">
              <div className="truncate text-[14px] font-semibold text-primary">{a.name}</div>
              <div className="truncate text-[11px] text-muted">
                {compactHomePath(a.globalDir ?? a.projectDir)}
              </div>
            </div>
            <span className="app-badge shrink-0">
              {t("agents.skillCount", { count: perAgent.get(a.id)?.length ?? 0 })}
            </span>
          </button>
        ))}
        {detected.length === 0 ? (
          <div className="app-panel-muted col-span-full px-6 py-14 text-center text-[13px] text-tertiary">
            {t("agents.noneDetected")}
          </div>
        ) : null}
      </div>

      {/* Not-detected agents stay browsable for reference. */}
      <details className="app-panel-muted px-4 py-3 text-[12px] text-tertiary">
        <summary className="cursor-pointer select-none outline-none">
          {t("agents.othersCount", {
            count: agents.length - detected.length,
          })}
        </summary>
        <div className="mt-3 grid grid-cols-2 gap-2 md:grid-cols-3">
          {agents
            .filter((a) => !a.detected)
            .map((a) => (
              <div key={a.id} className="flex items-center gap-2 text-muted">
                <AgentIcon agentKey={a.id} displayName={a.name} className="h-4 w-4" />
                <span className="truncate">{a.name}</span>
              </div>
            ))}
        </div>
      </details>
    </div>
  );
}
