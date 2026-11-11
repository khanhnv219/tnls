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

/** Who holds a local endpoint (`tnls_core::ports::ConflictOwner`). */
export type ConflictOwner =
  | { kind: "tunnel"; id: string; name: string; tags: string[]; pid: number | null }
  | { kind: "process"; pid: number | null; process: string | null };

export interface PortConflict {
  bind: string;
  port: number;
  owner: ConflictOwner;
}

/** A start refused because the local port is taken. `String(e)` is the message. */
export class PortConflictError extends Error {
  constructor(
    message: string,
    readonly conflict: PortConflict,
  ) {
    super(message);
    this.name = "PortConflictError";
  }
  toString(): string {
    return this.message;
  }
}

/**
 * Start-type commands reject with `{ kind: "portConflict" | "other", message }`.
 * Turn that into a PortConflictError or the plain message string other
 * callers already expect.
 */
async function startCall(cmd: string, args: Record<string, unknown>): Promise<TunnelStatus> {
  try {
    return await invoke<TunnelStatus>(cmd, args);
  } catch (e) {
    if (e && typeof e === "object" && "kind" in e && "message" in e) {
      const err = e as { kind: string; message: string; conflict?: PortConflict };
      if (err.kind === "portConflict" && err.conflict) {
        throw new PortConflictError(err.message, err.conflict);
      }
      throw err.message;
    }
    throw e;
  }
}

export async function startTunnel(id: string): Promise<TunnelStatus> {
  return startCall("start_tunnel", { id });
}

/** Conflict dialog: stop the tunnel holding the port, then start `id`. */
export async function takeOverPort(holderId: string, id: string): Promise<TunnelStatus> {
  return startCall("take_over_port", { holderId, id });
}

export async function stopTunnel(id: string): Promise<TunnelStatus> {
  return invoke("stop_tunnel", { id });
}

export async function restartTunnel(id: string): Promise<TunnelStatus> {
  return startCall("restart_tunnel", { id });
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

export type DiagStage = "sshAuth" | "localForward" | "destination";
export type DiagOutcome = "pass" | "fail" | "skipped" | "notRun";

/** `tnls_core::diagnose::StageReport`. */
export interface StageReport {
  stage: DiagStage;
  outcome: DiagOutcome;
  reason: string;
  detail: string[];
  at: string;
  elapsedMs: number;
}

export interface Diagnosis {
  tunnelId: string;
  tunnelName: string;
  startedAt: string;
  stages: StageReport[];
  summary: string;
}

/** Read-only staged diagnostics; never starts or stops the tunnel. */
export async function diagnoseTunnel(id: string): Promise<Diagnosis> {
  return invoke("diagnose_tunnel", { id });
}

/** Each stage as it finishes, before `diagnoseTunnel` resolves. */
export async function onDiagnoseStage(
  handler: (tunnelId: string, report: StageReport) => void,
): Promise<() => void> {
  return listen<{ tunnelId: string; report: StageReport }>("tunnel://diagnose-stage", (e) =>
    handler(e.payload.tunnelId, e.payload.report),
  );
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
