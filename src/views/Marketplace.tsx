import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Download, ExternalLink, Search } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import * as api from "../lib/api";
import type { MarketSkill } from "../lib/api";
import { useApp } from "../context/AppContext";
import { InstallDialog } from "../components/InstallDialog";
import { SearchInput } from "../components/SearchInput";
import { getErrorMessage } from "../lib/error";

/** Marketplace: skills.sh search (read-only HTTP) — installation always goes
 *  through the CLI. */
export function Marketplace() {
  const { t } = useTranslation();
  const { runtime, refreshGlobal } = useApp();
  const [query, setQuery] = useState("");
  const [owner, setOwner] = useState("");
  const [results, setResults] = useState<MarketSkill[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installTarget, setInstallTarget] = useState<{ source: string; skills: string[] | null } | null>(
    null,
  );
  const [freeSource, setFreeSource] = useState("");

  // Debounced search as the query changes (min 2 chars).
  useEffect(() => {
    const trimmed = query.trim();
    if (trimmed.length < 2) {
      setResults(null);
      setError(null);
      return;
    }
    const timer = setTimeout(async () => {
      setLoading(true);
      setError(null);
      try {
        setResults(await api.searchMarketplace(trimmed, owner.trim() || null));
      } catch (e) {
        setError(getErrorMessage(e, "Request failed"));
        setResults(null);
      } finally {
        setLoading(false);
      }
    }, 350);
    return () => clearTimeout(timer);
  }, [query, owner]);

  return (
    <div className="app-page">
      <div className="app-page-header">
        <h1 className="app-page-title">{t("marketplace.title")}</h1>
        <p className="app-page-subtitle">{t("marketplace.subtitle")}</p>
      </div>

      {/* Search */}
      <div className="app-toolbar">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t("marketplace.searchPlaceholder")}
          className="w-96"
        />
        <input
          type="text"
          value={owner}
          onChange={(e) => setOwner(e.target.value)}
          placeholder={t("marketplace.ownerFilter")}
          className="app-input w-44"
        />
      </div>

      {/* Free-form source */}
      <div className="app-panel flex flex-wrap items-center gap-2.5 p-3.5">
        <input
          type="text"
          value={freeSource}
          onChange={(e) => setFreeSource(e.target.value)}
          placeholder={t("marketplace.sourcePlaceholder")}
          className="app-input min-w-64 flex-1"
        />
        <button
          onClick={() => {
            if (freeSource.trim()) setInstallTarget({ source: freeSource.trim(), skills: null });
          }}
          className="app-button-secondary"
        >
          <Download className="h-3.5 w-3.5" />
          {t("marketplace.installFromSource")}
        </button>
      </div>

      {/* Results */}
      {error ? (
        <div className="app-panel-muted p-4 text-[13px] text-red-600 dark:text-red-400">
          {error}
        </div>
      ) : null}
      {loading ? (
        <div className="py-10 text-center text-[13px] text-muted">{t("common.loading")}</div>
      ) : results === null ? (
        <div className="app-panel-muted flex flex-col items-center gap-2 px-6 py-12 text-center">
          <Search className="h-6 w-6 text-muted" />
          <p className="text-[13px] text-tertiary">{t("marketplace.typeToSearch")}</p>
        </div>
      ) : results.length === 0 ? (
        <div className="app-panel-muted px-6 py-12 text-center text-[13px] text-tertiary">
          {t("marketplace.noResults")}
        </div>
      ) : (
        <div className="app-panel divide-y divide-border-subtle">
          {results.map((item) => (
            <div key={item.slug} className="flex items-center gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-medium text-primary">{item.name}</div>
                <div className="truncate text-[11px] text-muted">
                  {item.source} · {t("marketplace.installs", { count: item.installs })}
                </div>
              </div>
              <button
                onClick={() =>
                  openUrl(`https://skills.sh/${item.slug}`).catch(() => {})
                }
                title={t("marketplace.openOnSkillsSh")}
                className="rounded-md p-2 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary"
              >
                <ExternalLink className="h-3.5 w-3.5" />
              </button>
              <button
                onClick={() =>
                  setInstallTarget({ source: item.source, skills: [item.name] })
                }
                disabled={runtime ? !runtime.writesSupported : true}
                className="app-button-secondary shrink-0 px-3 py-1.5"
              >
                <Download className="h-3.5 w-3.5" />
                {t("installDialog.install")}
              </button>
            </div>
          ))}
        </div>
      )}

      {installTarget ? (
        <InstallDialog
          open
          source={installTarget.source}
          sourceLocked
          skills={installTarget.skills}
          onClose={() => setInstallTarget(null)}
          onInstalled={() => {
            refreshGlobal();
            toast.dismiss();
          }}
        />
      ) : null}
    </div>
  );
}
