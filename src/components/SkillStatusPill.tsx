import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import type { SkillStatus } from "../lib/api";

/** Status badge with the app's pill style. Colors: emerald = tracked,
 * amber = local/untracked, red = broken, muted = metadata unavailable. */
export function SkillStatusPill({ status }: { status: SkillStatus }) {
  const { t } = useTranslation();
  const [label, cls] = STATUS_STYLES[status](t);
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full border px-2.5 py-0.5 text-[11px] font-medium leading-4",
        cls,
      )}
    >
      {label}
    </span>
  );
}

const STATUS_STYLES: Record<SkillStatus, (t: (k: string) => string) => [string, string]> = {
  installed: (t) => [
    t("status.installed"),
    "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
  ],
  untracked: (t) => [
    t("status.untracked"),
    "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-400",
  ],
  "broken-link": (t) => [
    t("status.brokenLink"),
    "border-red-500/30 bg-red-500/10 text-red-700 dark:text-red-400",
  ],
  "metadata-unsupported": (t) => [
    t("status.metadataUnsupported"),
    "border-border-subtle bg-bg-secondary text-tertiary",
  ],
};
