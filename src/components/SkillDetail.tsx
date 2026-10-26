import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Copy, FolderOpen, RefreshCw, Share2, Trash2 } from "lucide-react";
import type { Scope, SkillView } from "../lib/api";
import * as api from "../lib/api";
import { DetailSheet } from "./DetailSheet";
import { SkillMarkdown } from "./SkillMarkdown";
import { SkillStatusPill } from "./SkillStatusPill";
import { AgentChips } from "./AgentChips";
import { ConfirmDialog } from "./ConfirmDialog";
import { ShareDialog } from "./ShareDialog";
import { getErrorMessage } from "../lib/error";
import { compactHomePath, cn } from "../utils";

interface Props {
  skill: SkillView | null;
  scope: Scope;
  onClose: () => void;
  /** Called after a write operation so the caller can refresh. */
  onChanged: () => void;
}

/** Full-screen detail sheet for one skill: SKILL.md plus safe actions. */
export function SkillDetail({ skill, scope, onClose, onChanged }: Props) {
  const { t } = useTranslation();
  const [doc, setDoc] = useState<string | null>(null);
  const [docTruncated, setDocTruncated] = useState(false);
  const [docError, setDocError] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState(false);
  const [updating, setUpdating] = useState(false);
  const [updateConfirm, setUpdateConfirm] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);

  useEffect(() => {
    if (!skill) return;
    setDoc(null);
    setDocError(null);
    api
      .readSkillDoc(scope, skill.path)
      .then((d) => {
        setDoc(d.markdown);
        setDocTruncated(d.truncated);
      })
      .catch((e) => setDocError(getErrorMessage(e, "Request failed")));
  }, [skill, scope]);

  if (!skill) return null;

  const copyPath = async () => {
    await writeText(skill.path);
    toast.success(t("common.copied"));
  };

  const reveal = async () => {
    try {
      await api.revealSkillFolder(scope, skill.path);
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    }
  };

  const handleUpdate = async () => {
    if (!skill) return;
    setUpdating(true);
    try {
      const result = await api.updateSkill({
        name: skill.name,
        scope,
        opId: api.newOpId(),
      });
      if (result.success) {
        toast.success(result.message);
      } else {
        toast.error(result.message);
      }
      onChanged();
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    } finally {
      setUpdating(false);
    }
  };

  const handleRemove = async () => {
    if (!skill) return;
    const result = await api.removeSkill({
      names: [skill.slug],
      scope,
      agents: null,
      opId: api.newOpId(),
    });
    if (result.success) {
      toast.success(result.message);
      onChanged();
      onClose();
    } else {
      toast.error(result.message);
      onChanged();
    }
  };

  const meta = (
    <div className="flex flex-wrap items-center gap-2">
      <SkillStatusPill status={skill.status} />
      {skill.source ? (
        <span className="rounded-full border border-border-subtle bg-bg-secondary px-2.5 py-0.5 text-[11px] text-tertiary">
          {skill.source}
        </span>
      ) : null}
      {skill.updatedAt ? (
        <span className="text-[12px] text-muted">
          {t("library.updatedAt", { date: new Date(skill.updatedAt).toLocaleString() })}
        </span>
      ) : null}
    </div>
  );

  return (
    <>
      <DetailSheet
        open
        title={skill.name}
        description={skill.description ?? undefined}
        meta={meta}
        onClose={onClose}
      >
        {/* Path + agents + actions */}
        <div className="mb-4 flex flex-col gap-3 rounded-xl border border-border-subtle bg-surface p-3.5">
          <div className="flex items-center gap-2 text-[12px] text-tertiary">
            <span className="shrink-0 text-muted">{t("library.path")}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-[11px]">
              {compactHomePath(skill.path)}
            </span>
            <button
              onClick={copyPath}
              title={t("common.copyPath")}
              className="rounded-md p-1.5 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary"
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
            <button
              onClick={reveal}
              title={t("common.revealFolder")}
              className="rounded-md p-1.5 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary"
            >
              <FolderOpen className="h-3.5 w-3.5" />
            </button>
          </div>
          <AgentChips agents={skill.agents} max={12} />
          <div className="flex items-center gap-2">
            {skill.isTrackedByVercel ? (
              <button
                onClick={() => setUpdateConfirm(true)}
                disabled={updating}
                className="app-button-secondary flex items-center gap-1.5"
              >
                <RefreshCw className={cn("h-3.5 w-3.5", updating && "animate-spin")} />
                {t("common.update")}
              </button>
            ) : (
              <span className="text-[12px] text-muted">
                {t("library.untrackedNoUpdate")}
              </span>
            )}
            {skill.status !== "broken-link" ? (
              <button
                onClick={() => setShareOpen(true)}
                className="app-button-secondary flex items-center gap-1.5"
              >
                <Share2 className="h-3.5 w-3.5" />
                {t("common.share")}
              </button>
            ) : null}
            <button
              onClick={() => setConfirmRemove(true)}
              className="flex items-center gap-1.5 rounded-lg border border-red-500/40 px-3 py-1.5 text-[13px] font-medium text-red-600 outline-none transition-colors hover:bg-red-500/10 disabled:opacity-50 dark:text-red-400"
            >
              <Trash2 className="h-3.5 w-3.5" />
              {t("common.uninstall")}
            </button>
          </div>
        </div>

        {/* SKILL.md */}
        {docError ? (
          <div className="rounded-xl border border-border-subtle bg-surface p-4 text-[13px] text-tertiary">
            {docError}
          </div>
        ) : doc === null ? (
          <div className="py-8 text-center text-[13px] text-muted">
            {t("common.loading")}
          </div>
        ) : (
          <>
            {docTruncated ? (
              <div className="mb-2 text-[12px] text-amber-600 dark:text-amber-400">
                {t("library.docTruncated")}
              </div>
            ) : null}
            <div className="rounded-xl border border-border-subtle bg-surface p-5">
              <SkillMarkdown content={doc} />
            </div>
          </>
        )}
      </DetailSheet>

      <ConfirmDialog
        open={confirmRemove}
        title={t("library.removeTitle")}
        message={t("library.removeMessage", { name: skill.name })}
        details={[compactHomePath(skill.path)]}
        confirmLabel={t("common.uninstall")}
        onClose={() => setConfirmRemove(false)}
        onConfirm={handleRemove}
      />

      <ConfirmDialog
        open={updateConfirm}
        tone="warning"
        title={t("library.updateTitle")}
        message={t("library.updateMessage", { name: skill.name })}
        confirmLabel={t("common.update")}
        lockWhileRunning
        onClose={() => setUpdateConfirm(false)}
        onConfirm={handleUpdate}
      />

      {skill.status !== "broken-link" ? (
        <ShareDialog
          open={shareOpen}
          skill={skill}
          scope={scope}
          onClose={() => setShareOpen(false)}
          onChanged={onChanged}
        />
      ) : null}
    </>
  );
}
