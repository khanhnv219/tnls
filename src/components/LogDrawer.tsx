import { useEffect, useState } from "react";
import { getTunnelLogs } from "../lib/api";

export function LogDrawer({
  tunnelId,
  tunnelName,
  onClose,
}: {
  tunnelId: string;
  tunnelName: string;
  onClose: () => void;
}) {
  const [lines, setLines] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getTunnelLogs(tunnelId, 300).then(
      (rows) => {
        if (!cancelled) setLines(rows);
      },
      (e) => {
        if (!cancelled) setError(String(e));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [tunnelId]);

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside
        className="drawer"
        onClick={(e) => e.stopPropagation()}
        aria-label={`Logs for ${tunnelName}`}
      >
        <header className="drawer-head">
          <h2>Logs — {tunnelName}</h2>
          <button onClick={onClose} aria-label="Close logs">
            ✕
          </button>
        </header>
        {error && <p className="error">{error}</p>}
        {lines === null && !error && <p className="muted">Loading…</p>}
        {lines !== null && lines.length === 0 && (
          <p className="muted">No log lines yet.</p>
        )}
        {lines !== null && lines.length > 0 && (
          <pre className="logs">{lines.join("\n")}</pre>
        )}
      </aside>
    </div>
  );
}
