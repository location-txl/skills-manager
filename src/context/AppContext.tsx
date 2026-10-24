import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import type {
  AgentStatus,
  CatalogResult,
  ProjectInfo,
  RuntimeStatus,
  SkillView,
} from "../lib/api";
import { getErrorMessage } from "../lib/error";

interface AppContextValue {
  runtime: RuntimeStatus | null;
  runtimeLoading: boolean;
  refreshRuntime: () => Promise<void>;

  globalCatalog: CatalogResult | null;
  globalLoading: boolean;
  refreshGlobal: () => Promise<void>;

  agents: AgentStatus[];
  refreshAgents: () => Promise<void>;

  project: ProjectInfo | null;
  projectCatalog: CatalogResult | null;
  projectLoading: boolean;
  openProject: (path: string) => Promise<ProjectInfo | null>;
  closeProject: () => Promise<void>;
  refreshProject: () => Promise<void>;

  appError: string | null;
  refreshing: boolean;
  refreshAll: () => Promise<void>;

  detailSkill: SkillView | null;
  openSkillDetail: (skill: SkillView) => void;
  closeSkillDetail: () => void;

  helpOpen: boolean;
  openHelp: () => void;
  closeHelp: () => void;
}

const AppContext = createContext<AppContextValue | null>(null);

export function AppProvider({ children }: { children: ReactNode }) {
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [runtimeLoading, setRuntimeLoading] = useState(true);
  const [globalCatalog, setGlobalCatalog] = useState<CatalogResult | null>(null);
  const [globalLoading, setGlobalLoading] = useState(true);
  const [agents, setAgents] = useState<AgentStatus[]>([]);
  const [project, setProject] = useState<ProjectInfo | null>(null);
  const [projectCatalog, setProjectCatalog] = useState<CatalogResult | null>(null);
  const [projectLoading, setProjectLoading] = useState(false);
  const [appError, setAppError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [detailSkill, setDetailSkill] = useState<SkillView | null>(null);
  const [helpOpen, setHelpOpen] = useState(false);

  // Re-apply the persisted CLI override before the first detection so the
  // very first status reflects the user's Settings choice. Pure UI config —
  // not business state.
  const overrideApplied = useRef(false);
  useEffect(() => {
    if (overrideApplied.current) return;
    overrideApplied.current = true;
    const cliPath = localStorage.getItem("skills-manager.cliPath");
    if (cliPath) {
      api.setCliPath(cliPath).catch(() => {});
    }
  }, []);

  const refreshRuntime = useCallback(async () => {
    setRuntimeLoading(true);
    try {
      const status = await api.getRuntimeStatus();
      setRuntime(status);
      setAppError(null);
    } catch (e) {
      setAppError(getErrorMessage(e, "Request failed"));
    } finally {
      setRuntimeLoading(false);
    }
  }, []);

  const refreshGlobal = useCallback(async () => {
    setGlobalLoading(true);
    try {
      const catalog = await api.listSkills({ kind: "global" });
      setGlobalCatalog(catalog);
    } catch (e) {
      setAppError(getErrorMessage(e, "Request failed"));
    } finally {
      setGlobalLoading(false);
    }
  }, []);

  const refreshAgents = useCallback(async () => {
    try {
      setAgents(await api.listAgentStatus({ kind: "global" }));
    } catch {
      // Agent listing is enrichment; a failure keeps the previous value.
    }
  }, []);

  const refreshProject = useCallback(async () => {
    setProject((current) => {
      if (!current) {
        setProjectCatalog(null);
        return current;
      }
      setProjectLoading(true);
      api
        .listSkills({ kind: "project", root: current.root })
        .then(setProjectCatalog)
        .catch((e) => setAppError(getErrorMessage(e, "Request failed")))
        .finally(() => setProjectLoading(false));
      return current;
    });
  }, []);

  const openProjectFn = useCallback(
    async (path: string): Promise<ProjectInfo | null> => {
      try {
        const info = await api.openProject(path);
        setProject(info);
        const catalog = await api.listSkills({
          kind: "project",
          root: info.root,
        });
        setProjectCatalog(catalog);
        setAppError(null);
        return info;
      } catch (e) {
        setAppError(getErrorMessage(e, "Request failed"));
        return null;
      }
    },
    [],
  );

  const closeProjectFn = useCallback(async () => {
    try {
      await api.closeProject();
    } catch {
      // The session project is process state; clearing locally is enough.
    }
    setProject(null);
    setProjectCatalog(null);
  }, []);

  const refreshAll = useCallback(async () => {
    setRefreshing(true);
    await Promise.allSettled([
      refreshRuntime(),
      refreshGlobal(),
      refreshAgents(),
      refreshProject(),
    ]);
    setRefreshing(false);
  }, [refreshRuntime, refreshGlobal, refreshAgents, refreshProject]);

  // Initial load.
  useEffect(() => {
    refreshAll();
  }, [refreshAll]);

  // The backend watcher (and every write operation) announces changes; a
  // short debounce keeps bursts of CLI writes from becoming refresh storms.
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    let disposed = false;
    const unlisten = listen(api.SKILLS_CHANGED_EVENT, () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        if (!disposed) refreshAll();
      }, 500);
    });
    return () => {
      disposed = true;
      if (timer) clearTimeout(timer);
      unlisten.then((fn) => fn());
    };
  }, [refreshAll]);

  const value = useMemo<AppContextValue>(
    () => ({
      runtime,
      runtimeLoading,
      refreshRuntime,
      globalCatalog,
      globalLoading,
      refreshGlobal,
      agents,
      refreshAgents,
      project,
      projectCatalog,
      projectLoading,
      openProject: openProjectFn,
      closeProject: closeProjectFn,
      refreshProject,
      appError,
      refreshing,
      refreshAll,
      detailSkill,
      openSkillDetail: setDetailSkill,
      closeSkillDetail: () => setDetailSkill(null),
      helpOpen,
      openHelp: () => setHelpOpen(true),
      closeHelp: () => setHelpOpen(false),
    }),
    [
      runtime,
      runtimeLoading,
      refreshRuntime,
      globalCatalog,
      globalLoading,
      refreshGlobal,
      agents,
      refreshAgents,
      project,
      projectCatalog,
      projectLoading,
      openProjectFn,
      closeProjectFn,
      refreshProject,
      appError,
      refreshing,
      refreshAll,
      detailSkill,
      helpOpen,
    ],
  );

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

// eslint-disable-next-line react-refresh/only-export-components
export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used within AppProvider");
  return ctx;
}
