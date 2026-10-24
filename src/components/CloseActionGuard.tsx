import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CloseActionDialog } from "./CloseActionDialog";
import * as api from "../lib/api";

const CLOSE_ACTION_KEY = "skills-manager.closeAction";
const TRAY_KEY = "skills-manager.trayEnabled";

/** Close-button behavior: exit or hide to tray. Pure UI preference, stored
 *  in localStorage — no backend settings store. */
export function CloseActionGuard() {
  const [dialogOpen, setDialogOpen] = useState(false);

  useEffect(() => {
    const unlisten = listen("window-close-requested", async () => {
      const trayEnabled = localStorage.getItem(TRAY_KEY) !== "false";
      if (!trayEnabled) {
        await api.appExit();
        return;
      }
      const pref = localStorage.getItem(CLOSE_ACTION_KEY);
      if (pref === "close") {
        await api.appExit();
      } else if (pref === "hide") {
        await api.hideToTray();
      } else {
        setDialogOpen(true);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleClose = async (remember: boolean) => {
    setDialogOpen(false);
    if (remember) localStorage.setItem(CLOSE_ACTION_KEY, "close");
    await api.appExit();
  };

  const handleHide = async (remember: boolean) => {
    setDialogOpen(false);
    if (remember) localStorage.setItem(CLOSE_ACTION_KEY, "hide");
    await api.hideToTray();
  };

  return (
    <CloseActionDialog
      open={dialogOpen}
      onCancel={() => setDialogOpen(false)}
      onClose={handleClose}
      onHide={handleHide}
    />
  );
}
