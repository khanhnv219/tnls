import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** What the window close button does. "ask" shows the close dialog. */
export type CloseAction = "ask" | "tray" | "quit";

const STORAGE_KEY = "tnls-close-action";
const HINT_KEY = "tnls-tray-hint-shown";

export function savedCloseAction(): CloseAction {
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (v === "tray" || v === "quit") return v;
  } catch {
    // Storage blocked: ask every time.
  }
  return "ask";
}

export function saveCloseAction(action: CloseAction): void {
  try {
    if (action === "ask") localStorage.removeItem(STORAGE_KEY);
    else localStorage.setItem(STORAGE_KEY, action);
  } catch {
    // Not persisted; applies to this session only.
  }
}

/** True once per install: whether to tell the user where the window went. */
export function takeTrayHint(): boolean {
  try {
    if (localStorage.getItem(HINT_KEY)) return false;
    localStorage.setItem(HINT_KEY, "1");
    return true;
  } catch {
    return false;
  }
}

/** Hide the window; tunnels keep running and the tray icon reopens it. */
export const hideToTray = (): Promise<void> => invoke("hide_to_tray");

/** Stop every tunnel and exit the app. */
export const quitApp = (): Promise<void> => invoke("quit_app");

/** The backend emits this instead of closing when the close button is pressed. */
export function onCloseRequested(handler: () => void): Promise<() => void> {
  return listen("app://close-requested", () => handler());
}
