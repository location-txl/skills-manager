import { useTranslation } from "react-i18next";
import { Check, FileText } from "lucide-react";
import type { SkillView } from "../lib/api";
import { cn } from "../utils";
import { AgentChips } from "./AgentChips";
import { SkillStatusPill } from "./SkillStatusPill";

interface Props {
  skill: SkillView;
  onOpen: (skill: SkillView) => void;
  selectionMode?: boolean;
  selected?: boolean;
  onToggleSelect?: (skill: SkillView) => void;
}

/** One skill card in the library/project grid. */
export function SkillCard({ skill, onOpen, selectionMode, selected, onToggleSelect }: Props) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => {
        if (selectionMode && onToggleSelect) {
          onToggleSelect(skill);
          return;
        }
        onOpen(skill);
      }}
      className={cn(
        "app-panel group relative flex h-full flex-col gap-3 p-4 text-left transition-colors outline-none",
        selected ? "border-accent-border bg-surface-active" : "hover:bg-surface-hover",
      )}
    >
      {selectionMode ? (
        <span
          className={cn(
            "absolute right-3 top-3 flex h-4 w-4 items-center justify-center rounded-md border",
            selected
              ? "border-accent-border bg-accent-dark text-white"
              : "border-border-subtle bg-surface",
          )}
        >
          {selected ? <Check className="h-3 w-3" /> : null}
        </span>
      ) : null}
      <div className="flex items-start justify-between gap-2 pr-6">
        <div className="flex min-w-0 items-center gap-2">
          <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-border-subtle bg-bg-secondary">
            <FileText className="h-4 w-4 text-muted" />
          </span>
          <div className="min-w-0">
            <div className="truncate text-[14px] font-semibold leading-5 text-primary">
              {skill.name}
            </div>
            <div className="truncate text-[11px] text-muted">{skill.slug}</div>
          </div>
        </div>
        {!selectionMode ? <SkillStatusPill status={skill.status} /> : null}
      </div>

      <p className="line-clamp-2 min-h-[2.5rem] text-[12px] leading-5 text-tertiary">
        {skill.description ?? t("library.noDescription")}
      </p>

      <div className="mt-auto flex flex-col gap-2">
        <AgentChips agents={skill.agents} max={4} />
        <div className="truncate text-[11px] text-faint">
          {skill.source ?? t("library.sourceUnknown")}
        </div>
      </div>
    </button>
  );
}
