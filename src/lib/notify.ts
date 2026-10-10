import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import type { ManagerEvent } from "./types";

/** Ask once at startup; no-op outside Tauri. */
export async function ensureNotificationPermission(): Promise<void> {
  try {
    if (await isPermissionGranted()) return;
    await requestPermission();
  } catch {
    // Browser preview without Tauri runtime: silently skip.
  }
}

/**
 * Decide whether a lifecycle event deserves a desktop notification.
 * Returns `{ title, body }` or null when quiet.
 */
export function notifyForEvent(
  event: ManagerEvent,
  lookup: (id: string) => string,
  prevState: string | undefined,
): { title: string; body: string } | null {
  const name = lookup(event.id);
  switch (event.kind.type) {
    case "failed":
      return { title: `Tunnel failed — ${name}`, body: event.kind.error };
    case "retryScheduled":
      return {
        title: `Retrying tunnel — ${name}`,
        body: `Attempt ${event.kind.attempt} in ${event.kind.delaySecs}s`,
      };
    case "healthChanged":
      return event.kind.healthy
        ? {
            title: `Tunnel recovered — ${name}`,
            body: "Local endpoint accepting connections again.",
          }
        : {
            title: `Tunnel degraded — ${name}`,
            body: "Process alive but local endpoint not accepting connections.",
          };
    case "stateChanged":
      if (
        event.kind.state === "running" &&
        prevState !== undefined &&
        prevState !== "running" &&
        prevState !== "starting"
      ) {
        return {
          title: `Tunnel running — ${name}`,
          body: "Connection established.",
        };
      }
      return null;
    default:
      return null;
  }
}

export function fireNotification(title: string, body: string): void {
  try {
    sendNotification({ title, body });
  } catch {
    // Preview / denied permission: ignore.
  }
}
