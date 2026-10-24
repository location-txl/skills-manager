import { Link, useLocation } from "react-router-dom";
import { useTranslation } from "react-i18next";
import {
  LayoutDashboard,
  Layers,
  Bot,
  FolderOpen,
  Store,
  Settings,
  CircleAlert,
} from "lucide-react";
import { cn } from "../utils";
import { IS_MACOS } from "../lib/platform";
import { useApp } from "../context/AppContext";

export function Sidebar() {
  const { t } = useTranslation();
  const location = useLocation();
  const { runtime, project } = useApp();

  const navItems = [
    { name: t("nav.overview"), path: "/", icon: LayoutDashboard },
    { name: t("nav.library"), path: "/library", icon: Layers },
    { name: t("nav.agents"), path: "/agents", icon: Bot },
    { name: t("nav.project"), path: "/project", icon: FolderOpen },
    { name: t("nav.marketplace"), path: "/marketplace", icon: Store },
  ];

  return (
    <div className="w-[220px] flex-shrink-0 bg-bg-secondary border-r border-border-subtle h-full flex flex-col select-none relative z-10">
      {/* Only macOS needs the extra space for overlaid window controls. */}
      <div className={cn("shrink-0", IS_MACOS ? "h-[38px]" : "h-2.5")} />
      {/* App title */}
      <div className="px-4 pb-3 shrink-0">
        <div className="text-[17px] font-semibold tracking-tight text-primary">
          {t("app.title")}
        </div>
        <div className="mt-0.5 text-[12px] text-muted truncate">
          {t("app.subtitle")}
        </div>
      </div>

      {/* Nav */}
      <div className="px-2.5 space-y-0.5 shrink-0">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive =
            item.path === "/"
              ? location.pathname === "/"
              : location.pathname.startsWith(item.path);
          return (
            <Link
              key={item.path}
              to={item.path}
              className={cn(
                "flex items-center gap-2.5 px-2.5 py-[7px] rounded-md text-[14px] leading-5 font-medium transition-colors outline-none",
                isActive
                  ? "bg-surface-active text-primary"
                  : "text-tertiary hover:text-secondary hover:bg-surface-hover",
              )}
            >
              <Icon
                className={cn("w-4 h-4 shrink-0", isActive ? "text-accent" : "text-muted")}
              />
              {item.name}
            </Link>
          );
        })}
      </div>

      <div className="mx-3 mt-3.5 mb-2.5 border-t border-border-subtle" />

      {/* Session state: the open project (process state, not a registry). */}
      <div className="px-2.5 flex-1 overflow-y-auto scrollbar-hide min-h-0 space-y-1">
        {project ? (
          <Link
            to="/project"
            className="block rounded-md px-2.5 py-2 transition-colors outline-none hover:bg-surface-hover"
          >
            <div className="text-[12px] font-semibold text-muted">
              {t("sidebar.openProject")}
            </div>
            <div className="mt-0.5 truncate text-[13px] text-secondary">
              {project.name}
            </div>
          </Link>
        ) : null}

        {runtime?.degraded ? (
          <Link
            to="/settings"
            className="flex items-start gap-2 rounded-md px-2.5 py-2 outline-none hover:bg-surface-hover"
          >
            <CircleAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-500" />
            <span className="text-[12px] leading-4 text-tertiary">
              {t("sidebar.runtimeAttention")}
            </span>
          </Link>
        ) : null}
      </div>

      {/* Settings */}
      <div className="p-2.5 border-t border-border-subtle shrink-0">
        <Link
          to="/settings"
          className={cn(
            "flex items-center gap-2.5 px-2.5 py-[7px] rounded-md text-[14px] leading-5 font-medium transition-colors outline-none",
            location.pathname === "/settings"
              ? "bg-surface-active text-primary"
              : "text-tertiary hover:text-secondary hover:bg-surface-hover",
          )}
        >
          <Settings
            className={cn(
              "w-4 h-4 shrink-0",
              location.pathname === "/settings" ? "text-accent" : "text-muted",
            )}
          />
          {t("nav.settings")}
        </Link>
      </div>
    </div>
  );
}
