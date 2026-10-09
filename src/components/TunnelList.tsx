import { useState } from "react";
import type { TunnelRow } from "../lib/types";
import { StatusBadge } from "./StatusBadge";

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
  onShowLogs: (row: TunnelRow) => void;
}) {
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  if (rows.length === 0) {
    return (
      <div className="empty">
        <p>No tunnels yet. Create one to stop typing ssh commands.</p>
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
            const detail =
              row.status.lastError ??
              (row.status.pid != null ? `pid ${row.status.pid}` : "—");
            return (
              <tr key={row.id} className={`row-${row.status.state}`}>
                <td className="name">{row.name}</td>
                <td>{row.mode}</td>
                <td>
                  <code>
                    {row.bindAddress}:{row.localPort}
                  </code>
                </td>
                <td>
                  <code>{remoteLabel(row)}</code>
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
