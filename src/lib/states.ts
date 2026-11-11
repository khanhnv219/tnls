import type { TunnelState } from "./types";

const STARTABLE: ReadonlySet<TunnelState> = new Set(["stopped", "failed", "unknown"]);
const STOPPABLE: ReadonlySet<TunnelState> = new Set(["starting", "running", "degraded", "unknown"]);

export const canStart = (state: TunnelState) => STARTABLE.has(state);
export const canStop = (state: TunnelState) => STOPPABLE.has(state);
