import { useTranslation } from "react-i18next";
import { AgentIcon } from "./AgentIcon";
import { cn } from "../utils";
import type { AgentExposure } from "../lib/api";

/** Row of agent chips; the icon is tinted by how the agent sees the skill. */
export function AgentChips({
  agents,
  max = 6,
  className,
}: {
  agents: AgentExposure[];
  max?: number;
  className?: string;
}) {
  const { t } = useTranslation();
  if (agents.length === 0) {
    return (
      <span className={cn("text-[12px] text-muted", className)}>
        {t("agents.notLinked")}
      </span>
    );
  }
  const shown = agents.slice(0, max);
  const rest = agents.length - shown.length;
  return (
    <div className={cn("flex flex-wrap items-center gap-1.5", className)}>
      {shown.map((agent) => (
        <span
          key={`${agent.id}-${agent.exposure}`}
          title={exposureTitle(agent, t)}
          className="inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-bg-secondary py-0.5 pl-0.5 pr-2 text-[11px] text-tertiary"
        >
          <AgentIcon
            agentKey={agent.id}
            displayName={agent.name}
            className="h-4 w-4 rounded-full"
          />
          <span className="max-w-[120px] truncate">{agent.name}</span>
          <span
            className={cn(
              "h-1.5 w-1.5 shrink-0 rounded-full",
              EXPOSURE_DOT[agent.exposure],
            )}
          />
        </span>
      ))}
      {rest > 0 ? (
        <span className="text-[11px] text-muted">+{rest}</span>
      ) : null}
    </div>
  );
}

function exposureTitle(agent: AgentExposure, t: (k: string) => string): string {
  const how = {
    shared: t("exposure.shared"),
    symlink: t("exposure.symlink"),
    copy: t("exposure.copy"),
    native: t("exposure.native"),
    unknown: t("exposure.unknown"),
  }[agent.exposure];
  return `${agent.name} · ${how}`;
}

const EXPOSURE_DOT: Record<AgentExposure["exposure"], string> = {
  shared: "bg-sky-500",
  symlink: "bg-emerald-500",
  copy: "bg-violet-500",
  native: "bg-amber-500",
  unknown: "bg-muted",
};
