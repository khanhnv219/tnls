import { useEffect, useRef, useState } from "react";
import type { TunnelState } from "../lib/types";
import { Icon, type IconName } from "./Icon";

export const STATE_LABEL: Record<TunnelState, string> = {
  stopped: "Stopped",
  starting: "Starting",
  running: "Running",
  degraded: "Degraded",
  stopping: "Stopping",
  failed: "Failed",
  unknown: "Unknown",
};

const STATE_ICON: Record<TunnelState, IconName> = {
  stopped: "circle",
  starting: "loader",
  running: "dot",
  degraded: "alert",
  stopping: "loader",
  failed: "alert",
  unknown: "circle",
};

/**
 * State is carried by color, icon, and word (the row rail is the fourth
 * channel). A state change flashes the badge so the transition is noticed.
 */
export function StatusBadge({
  state,
  restartCount = 0,
}: {
  state: TunnelState;
  restartCount?: number;
}) {
  const prev = useRef(state);
  const [changing, setChanging] = useState(false);

  useEffect(() => {
    if (prev.current === state) return;
    prev.current = state;
    setChanging(true);
    const t = setTimeout(() => setChanging(false), 300);
    return () => clearTimeout(t);
  }, [state]);

  const transitional = state === "starting" || state === "stopping";

  return (
    <span
      className={`badge${changing ? " is-changing" : ""}`}
      data-state={state}
      title={`Runtime state: ${state}`}
    >
      {transitional ? (
        <Icon name="loader" size={13} className="spinner" />
      ) : (
        <Icon name={STATE_ICON[state]} size={13} className="badge__icon" />
      )}
      <span className="badge__text">{STATE_LABEL[state]}</span>
      {restartCount > 0 && (
        <span className="badge__retry" title="Retry attempts">
          ↻{restartCount}
        </span>
      )}
    </span>
  );
}
