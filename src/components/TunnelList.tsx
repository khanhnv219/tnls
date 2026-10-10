import { useEffect, useState } from "react";
import type { TunnelRow } from "../lib/types";
import { StatusBadge } from "./StatusBadge";

function formatUptime(startedAt: string | null | undefined, now: number): string | null {
  if (!startedAt) return null;
  const start = new Date(startedAt).getTime();
  if (Number.isNaN(start)) return null;
  let secs = Math.max(0, Math.floor((now - start) / 1000));
  const h = Math.floor(secs / 3600);
  secs %= 3600;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function remoteLabel(row: TunnelRow): string {
  if (row.mode === "dynamic") return `SOCKS via ${row.sshTarget}`;
  return `${row.remoteHost ?? "?"}:${row.remotePort ?? "?"}`;
}

const STARTABLE: Record<string, true> = {
  stopped: true,
  failed: true,
  unknown: true,
};

const STOPPABLE: Record<string, true> = {
  starting: true,
  running: true,
  degraded: true,
  unknown: true,
};

function canStart(row: TunnelRow): boolean {
  return STARTABLE[row.status.state] === true;
}

function canStop(row: TunnelRow): boolean {
  return STOPPABLE[row.status.state] === true;
}

export function TunnelList({
  rows,
  pending,
  actionError,
  onStart,
  onStop,
  onRestart,
  onDelete,
  onEdit,
  onDuplicate,
  onShowLogs,
}: {
  rows: TunnelRow[];
  pending: Record<string, boolean>;
  actionError: string | null;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onRestart: (id: string) => void;
  onDelete: (id: string) => void;
  onEdit: (row: TunnelRow) => void;
  onDuplicate: (row: TunnelRow) => void;
  onShowLogs: (row: TunnelRow) => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    if (copied === null) return;
    const t = setTimeout(() => setCopied(null), 1400);
    return () => clearTimeout(t);
  }, [copied]);

  if (rows.length === 0) {
    return (
      <div className="empty" role="status" aria-live="polite">
        <h3 style={{ margin: "0 0 8px" }}>No tunnels yet</h3>
        <p style={{ margin: "0 0 14px" }}>
          Create your first tunnel — it is a typed profile, not a raw shell
          command. Start with a local forward like
          {" "}
          <code>127.0.0.1:5433 → db.internal:5432</code>
          {" "}
          via your bastion.
        </p>
        <p className="muted" style={{ fontSize: 13 }}>
          Tip: use an <code>~/.ssh/config</code> alias as SSH target, press
          {" "}
          <kbd>/</kbd>
          {" "}
          to search, <kbd>Ctrl+N</kbd> for a new tunnel.
        </p>
      </div>
    );
  }

  return (
    <div className="table-wrap">
      {actionError && (
        <p className="error" role="alert">
          {actionError}
        </p>
      )}
      <table className="tunnels">
        <thead>
          <tr>
            <th>Name</th>
            <th>Mode</th>
            <th>Local</th>
            <th>Remote</th>
            <th>Status</th>
            <th>Detail</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const busy = pending[row.id] ?? false;
            const local = `${row.bindAddress}:${row.localPort}`;
            const remote = remoteLabel(row);
            const uptime = formatUptime(row.status.startedAt, now);
            const detail = row.status.lastError
              ? row.status.lastError
              : row.status.pid != null
                ? `pid ${row.status.pid}${uptime ? ` · ${uptime}` : ""}`
                : uptime ?? "—";
            return (
              <tr key={row.id} className={`row-${row.status.state}`}>
                <td className="name">{row.name}</td>
                <td>{row.mode}</td>
                <td>
                  <code
                    className="copyable"
                    title={copied === local ? "Copied!" : "Click to copy"}
                    onClick={() => {
                      navigator.clipboard.writeText(local).then(
                        () => setCopied(local),
                        () => setCopied(null),
                      );
                    }}
                  >
                    {local}
                  </code>
                  {copied === local && <span className="copied-hint">copied</span>}
                </td>
                <td>
                  <code
                    className="copyable"
                    title={copied === remote ? "Copied!" : "Click to copy"}
                    onClick={() => {
                      navigator.clipboard.writeText(remote).then(
                        () => setCopied(remote),
                        () => setCopied(null),
                      );
                    }}
                  >
                    {remote}
                  </code>
                  {copied === remote && <span className="copied-hint">copied</span>}
                </td>
                <td>
                  <StatusBadge state={row.status.state} />
                  {row.status.restartCount > 0 && (
                    <span className="muted" title="Retry attempts">
                      {" "}
                      ↻{row.status.restartCount}
                    </span>
                  )}
                </td>
                <td className="detail" title={detail}>
                  {detail}
                </td>
                <td className="actions">
                  {canStart(row) && (
                    <button disabled={busy} onClick={() => onStart(row.id)}>
                      Start
                    </button>
                  )}
                  {canStop(row) && (
                    <button disabled={busy} onClick={() => onStop(row.id)}>
                      Stop
                    </button>
                  )}
                  <button disabled={busy} onClick={() => onRestart(row.id)}>
                    Restart
                  </button>
                  <button onClick={() => onEdit(row)}>Edit</button>
                  <button onClick={() => onDuplicate(row)}>Duplicate</button>
                  <button onClick={() => onShowLogs(row)}>Logs</button>
                  {confirmDelete === row.id ? (
                    <>
                      <button
                        className="danger"
                        disabled={busy}
                        onClick={() => {
                          setConfirmDelete(null);
                          onDelete(row.id);
                        }}
                      >
                        Confirm
                      </button>
                      <button onClick={() => setConfirmDelete(null)}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <button
                      className="danger-ghost"
                      onClick={() => setConfirmDelete(row.id)}
                    >
                      Delete
                    </button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
