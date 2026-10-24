import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import {
  CircleCheck,
  CircleAlert,
  FolderOpen,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import { useApp } from "../context/AppContext";
import { useTheme, type Theme } from "../hooks/useTheme";
import { applyTextSize } from "../lib/textScale";
import * as api from "../lib/api";
import { getLastCliError } from "../lib/api";
import { getErrorMessage } from "../lib/error";
import { compactHomePath, cn } from "../utils";

const TEXT_SIZES = ["small", "default", "large", "xlarge"] as const;

/** Runtime diagnostics + pure UI preferences. No central repo, no backup,
 *  no sync settings — those concepts are gone with the old store. */
export function Settings() {
  const { t } = useTranslation();
  const { i18n } = useTranslation();
  const { runtime, runtimeLoading, refreshRuntime, agents } = useApp();
  const { theme, setTheme } = useTheme();
  const [textSize, setTextSize] = useState(
    () => localStorage.getItem("skills-manager.textSize") ?? "default",
  );
  const [language, setLanguage] = useState(() => localStorage.getItem("language") ?? "en");
  const [cliOverride, setCliOverride] = useState(
    () => localStorage.getItem("skills-manager.cliPath") ?? "",
  );
  const [lastError, setLastError] = useState<string | null>(null);

  useEffect(() => {
    getLastCliError().then(setLastError);
  }, [runtime]);

  const pickCli = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      title: t("settings.pickCliTitle"),
    });
    if (typeof selected !== "string") return;
    setCliOverride(selected);
    localStorage.setItem("skills-manager.cliPath", selected);
    try {
      await api.setCliPath(selected);
      await refreshRuntime();
      toast.success(t("settings.cliPathUpdated"));
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    }
  };

  const clearCli = async () => {
    setCliOverride("");
    localStorage.removeItem("skills-manager.cliPath");
    try {
      await api.setCliPath(null);
      await refreshRuntime();
    } catch (e) {
      toast.error(getErrorMessage(e, "Request failed"));
    }
  };

  const changeTextSize = (size: string) => {
    setTextSize(size);
    localStorage.setItem("skills-manager.textSize", size);
    applyTextSize(size);
  };

  const changeLanguage = (lang: string) => {
    setLanguage(lang);
    localStorage.setItem("language", lang);
    i18n.changeLanguage(lang);
  };

  const detectedAgents = agents.filter((a) => a.detected);

  return (
    <div className="app-page app-page-narrow">
      <div className="app-page-header flex items-start justify-between">
        <div>
          <h1 className="app-page-title">{t("settings.title")}</h1>
          <p className="app-page-subtitle">{t("settings.subtitle")}</p>
        </div>
        <button
          onClick={refreshRuntime}
          disabled={runtimeLoading}
          className="app-toolbar-button app-toolbar-button-secondary"
        >
          <RefreshCw className={cn("h-3.5 w-3.5", runtimeLoading && "animate-spin")} />
          {t("settings.reDetect")}
        </button>
      </div>

      {/* Runtime */}
      <section className="app-panel p-5">
        <div className="mb-3 flex items-center gap-2">
          {runtime?.degraded ? (
            <CircleAlert className="h-4 w-4 text-amber-500" />
          ) : (
            <CircleCheck className="h-4 w-4 text-emerald-500" />
          )}
          <h2 className="text-[13px] font-semibold text-primary">{t("settings.runtime")}</h2>
        </div>
        <dl className="space-y-2 text-[12px]">
          <Row label={t("settings.node")} value={runtime ? `${runtime.nodeVersion ?? "—"} · ${runtime.nodePath ?? "—"}` : "…"} />
          <Row label={t("settings.cli")} value={runtime ? `${runtime.cliVersion ?? "—"} · ${runtime.cliPath ?? "—"}` : "…"} />
          <Row label={t("settings.globalDir")} value={compactHomePath(runtime?.globalSkillsDir ?? "")} />
          <Row label={t("settings.lockPath")} value={compactHomePath(runtime?.globalLockPath ?? "")} />
          <Row
            label={t("settings.writes")}
            value={runtime?.writesSupported ? t("settings.writesOk") : t("settings.writesBlocked")}
          />
        </dl>
        {runtime?.notes.length ? (
          <ul className="mt-3 space-y-1.5 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3">
            {runtime.notes.map((note, i) => (
              <li key={i} className="text-[12px] leading-5 text-amber-700 dark:text-amber-400">
                {note}
              </li>
            ))}
          </ul>
        ) : null}
        {lastError ? (
          <div className="mt-3 rounded-lg border border-red-500/30 bg-red-500/5 p-3 text-[12px] leading-5 text-red-700 dark:text-red-400">
            <div className="mb-1 font-medium">{t("settings.lastCliError")}</div>
            {lastError}
          </div>
        ) : null}
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <button onClick={pickCli} className="app-button-secondary px-3 py-1.5">
            <FolderOpen className="h-3.5 w-3.5" />
            {t("settings.pickCli")}
          </button>
          {cliOverride ? (
            <button onClick={clearCli} className="app-button-secondary px-3 py-1.5">
              <RotateCcw className="h-3.5 w-3.5" />
              {t("settings.clearCliOverride")}
            </button>
          ) : null}
          {cliOverride ? (
            <span className="truncate font-mono text-[11px] text-muted">{cliOverride}</span>
          ) : null}
        </div>
      </section>

      {/* Detected agents */}
      <section className="app-panel p-5">
        <h2 className="mb-3 text-[13px] font-semibold text-primary">
          {t("settings.detectedAgents", { count: detectedAgents.length })}
        </h2>
        {detectedAgents.length === 0 ? (
          <p className="text-[12px] text-muted">{t("agents.noneDetected")}</p>
        ) : (
          <div className="flex flex-wrap gap-1.5">
            {detectedAgents.map((a) => (
              <span key={a.id} className="app-badge">
                {a.name}
              </span>
            ))}
          </div>
        )}
      </section>

      {/* Preferences (pure UI, localStorage) */}
      <section className="app-panel p-5">
        <h2 className="mb-3 text-[13px] font-semibold text-primary">{t("settings.preferences")}</h2>
        <div className="space-y-4">
          <div className="flex items-center justify-between gap-4">
            <span className="text-[12px] text-tertiary">{t("settings.themeLabel")}</span>
            <div className="app-segmented">
              {(["system", "light", "dark"] as Theme[]).map((mode) => (
                <button
                  key={mode}
                  onClick={() => setTheme(mode)}
                  className={cn(
                    "app-segmented-button",
                    theme === mode && "app-segmented-button-active",
                  )}
                >
                  {t(`settings.theme.${mode}`)}
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center justify-between gap-4">
            <span className="text-[12px] text-tertiary">{t("settings.textSizeLabel")}</span>
            <div className="app-segmented">
              {TEXT_SIZES.map((size) => (
                <button
                  key={size}
                  onClick={() => changeTextSize(size)}
                  className={cn(
                    "app-segmented-button",
                    textSize === size && "app-segmented-button-active",
                  )}
                >
                  {t(`settings.textSize.${size}`)}
                </button>
              ))}
            </div>
          </div>
          <div className="flex items-center justify-between gap-4">
            <span className="text-[12px] text-tertiary">{t("settings.language")}</span>
            <div className="app-segmented">
              {[
                ["en", "English"],
                ["zh", "中文"],
                ["zh-TW", "繁體"],
                ["es", "Español"],
                ["ko", "한국어"],
              ].map(([code, label]) => (
                <button
                  key={code}
                  onClick={() => changeLanguage(code)}
                  className={cn(
                    "app-segmented-button",
                    language === code && "app-segmented-button-active",
                  )}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* About */}
      <section className="app-panel-muted p-5">
        <h2 className="mb-2 text-[13px] font-semibold text-primary">{t("settings.about")}</h2>
        <p className="text-[12px] leading-5 text-tertiary">{t("settings.aboutBody")}</p>
        <p className="mt-2 text-[11px] leading-5 text-muted">{t("settings.aboutVercel")}</p>
      </section>
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-baseline gap-3">
      <dt className="w-28 shrink-0 text-muted">{label}</dt>
      <dd className="min-w-0 flex-1 truncate font-mono text-[11px] text-secondary">{value}</dd>
    </div>
  );
}
