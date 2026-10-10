export type TunnelMode = "local" | "remote" | "dynamic";

export type TunnelState =
  | "stopped"
  | "starting"
  | "running"
  | "degraded"
  | "stopping"
  | "failed"
  | "unknown";

export interface TunnelConfig {
  id: string;
  name: string;
  mode: TunnelMode;
  sshTarget: string;
  bindAddress: string;
  localPort: number;
  remoteHost?: string | null;
  remotePort?: number | null;
  identityFile?: string | null;
  proxyJump?: string | null;
  autoStart: boolean;
  keepAliveInterval: number;
  createdAt: string;
  updatedAt: string;
}

export interface TunnelInput {
  name: string;
  mode: TunnelMode;
  sshTarget: string;
  bindAddress?: string | null;
  localPort: number;
  remoteHost?: string | null;
  remotePort?: number | null;
  identityFile?: string | null;
  proxyJump?: string | null;
  autoStart?: boolean | null;
  keepAliveInterval?: number | null;
}

export interface TunnelStatus {
  id: string;
  state: TunnelState;
  pid?: number | null;
  startedAt?: string | null;
  lastError?: string | null;
  restartCount: number;
}

export type ManagerEventKind =
  | { type: "started"; pid: number }
  | { type: "stateChanged"; state: TunnelState }
  | { type: "retryScheduled"; attempt: number; delaySecs: number }
  | { type: "failed"; error: string }
  | { type: "stopped" }
  | { type: "healthChanged"; healthy: boolean };

export interface ManagerEvent {
  id: string;
  kind: ManagerEventKind;
  at: string;
}

export interface TunnelRow extends TunnelConfig {
  status: TunnelStatus;
}
