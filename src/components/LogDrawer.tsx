import { useEffect, useRef, useState } from "react";
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
  const [live, setLive] = useState(true);
  const preRef = useRef<HTMLPreElement>(null);

  useEffect(() => {
    let cancelled = false;
    async function fetch(): Promise<void> {
      try {
        const rows = await getTunnelLogs(tunnelId, 400);
        if (!cancelled) {
          setLines(rows);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    }
    fetch();
    if (!live) return () => { cancelled = true; };
    const id = setInterval(fetch, 2000);
    return () => { cancelled = true; clearInterval(id); };
  }, [tunnelId, live]);

  useEffect(() => {
    if (live && preRef.current) {
      preRef.current.scrollTop = preRef.current.scrollHeight;
    }
  }, [lines, live]);

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside
        className="drawer"
        onClick={(e) => e.stopPropagation()}
        aria-label={`Logs for ${tunnelName}`}
      >
        <header className="drawer-head">
          <h2>Logs — {tunnelName}</h2>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <label style={{ fontSize: 13, display: "flex", gap: 6, alignItems: "center" }}>
              <input type="checkbox" checked={live} onChange={(e) => setLive(e.target.checked)} />
              Live
            </label>
            <button
              className="ghost"
              onClick={async () => {
                try {
                  const rows = await getTunnelLogs(tunnelId, 400);
                  setLines(rows);
                  setError(null);
                } catch (e) { setError(String(e)); }
              }}
            >
              Refresh
            </button>
            <button onClick={onClose} aria-label="Close logs">✕</button>
          </div>
        </header>
        {error && <p className="error">{error}</p>}
        {lines === null && !error && <p className="muted">Loading…</p>}
        {lines !== null && lines.length === 0 && (
          <p className="muted">No log lines yet.</p>
        )}
        {lines !== null && lines.length > 0 && (
          <pre ref={preRef} className="logs">{lines.join("\n")}</pre>
        )}
      </aside>
    </div>
  );
}
