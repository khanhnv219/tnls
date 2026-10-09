import type { TunnelState } from "../lib/types";

const LABELS: Record<TunnelState, string> = {
  stopped: "Stopped",
  starting: "Starting",
  running: "Running",
  degraded: "Degraded",
  stopping: "Stopping",
  failed: "Failed",
  unknown: "Unknown",
};

export function StatusBadge({ state }: { state: TunnelState }) {
  return (
    <span className={`badge badge-${state}`} title={`State: ${state}`}>
      <span className="dot" aria-hidden />
      {LABELS[state]}
    </span>
  );
}
