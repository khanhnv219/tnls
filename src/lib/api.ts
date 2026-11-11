import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { LogLine } from "./logs";
import type {
  ManagerEvent,
  TunnelConfig,
  TunnelInput,
  TunnelStatus,
} from "./types";

export async function listTunnels(): Promise<TunnelConfig[]> {
  return invoke("list_tunnels");
}

export async function createTunnel(input: TunnelInput): Promise<TunnelConfig> {
  return invoke("create_tunnel", { input });
}

export async function updateTunnel(
  id: string,
  input: TunnelInput,
): Promise<TunnelConfig> {
  return invoke("update_tunnel", { id, input });
}

export async function deleteTunnel(id: string): Promise<void> {
  return invoke("delete_tunnel", { id });
}

export async function startTunnel(id: string): Promise<TunnelStatus> {
  return invoke("start_tunnel", { id });
}

export async function stopTunnel(id: string): Promise<TunnelStatus> {
  return invoke("stop_tunnel", { id });
}

export async function restartTunnel(id: string): Promise<TunnelStatus> {
  return invoke("restart_tunnel", { id });
}

export async function getTunnelLogs(
  id: string,
  limit?: number,
): Promise<LogLine[]> {
  return invoke("get_tunnel_logs", { id, limit: limit ?? null });
}

export interface CommandPreview {
  /** Resolved ssh binary path. */
  program: string;
  /** Arguments after the program; missing values appear as `<placeholders>`. */
  args: string[];
}

/** Display-only: the argv the backend would spawn for this (draft) input. */
export async function previewSshCommand(input: TunnelInput): Promise<CommandPreview> {
  return invoke("preview_ssh_command", { input });
}

export async function getRuntimeStatus(): Promise<TunnelStatus[]> {
  return invoke("get_runtime_status");
}

export async function getPassphraseState(id: string): Promise<boolean> {
  return invoke("get_passphrase_state", { id });
}

export async function setPassphrase(id: string, passphrase: string): Promise<void> {
  return invoke("set_passphrase", { id, passphrase });
}

export async function deletePassphrase(id: string): Promise<void> {
  return invoke("delete_passphrase", { id });
}

export async function exportTunnelsJson(): Promise<string> {
  return invoke("export_tunnels_json");
}

export async function importTunnelsJson(json: string): Promise<TunnelConfig[]> {
  return invoke("import_tunnels_json", { json });
}

export async function importSshConfig(path?: string | null): Promise<TunnelConfig[]> {
  return invoke("import_ssh_config", { path: path ?? null });
}

const EVENT_TOPICS = [
  "tunnel://started",
  "tunnel://state-changed",
  "tunnel://retry-scheduled",
  "tunnel://failed",
  "tunnel://stopped",
  "tunnel://health-changed",
] as const;

/** Subscribe to all tunnel lifecycle events; returns an unsubscribe fn. */
export async function onTunnelEvent(
  handler: (event: ManagerEvent) => void,
): Promise<() => void> {
  const unlistens: UnlistenFn[] = [];
  for (const topic of EVENT_TOPICS) {
    unlistens.push(await listen<ManagerEvent>(topic, (e) => handler(e.payload)));
  }
  return () => unlistens.forEach((fn) => fn());
}
