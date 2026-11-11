import { useEffect, useRef, useState } from "react";
import { canStart, canStop } from "../lib/states";
import type { TunnelMode, TunnelRow } from "../lib/types";
import { Icon } from "./Icon";
import { Menu, type MenuEntry } from "./Menu";
import { StatusBadge } from "./StatusBadge";

const MODE_LABEL: Record<TunnelMode, string> = {
  local: "Local",
  remote: "Remote",
  dynamic: "Dynamic",
};

const MODE_FLAG: Record<TunnelMode, string> = {
  local: "-L",
  remote: "-R",
  dynamic: "-D",
};

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
  if (row.mode === "dynamic") return `SOCKS ${row.bindAddress}:${row.localPort}`;
  return `${row.remoteHost ?? "?"}:${row.remotePort ?? "?"}`;
}

/**
 * The detail column says what is happening right now. A failed or degraded
 * row shows its error, never an uptime: uptime on a dead row misleads.
 */
function detailText(row: TunnelRow, now: number): { text: string; error: boolean } {
  const { state, lastError, pid, startedAt } = row.status;
  if (lastError && (state === "failed" || state === "degraded" || state === "unknown")) {
    return { text: lastError, error: true };
  }
  if (state === "starting") return { text: "establishing channel…", error: false };
  if (state === "stopping") return { text: "closing channels…", error: false };
  if (pid != null) {
    const up = formatUptime(startedAt, now);
    return { text: `pid ${pid}${up ? ` · up ${up}` : ""}`, error: false };
  }
  return { text: "—", error: false };
}

function AddressCell({
  value,
  label,
  muted,
}: {
  value: string;
  label: string;
  muted?: boolean;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1200);
    return () => clearTimeout(t);
  }, [copied]);

  return (
    <div className={`addr${muted ? " addr--muted" : ""}`}>
      <span className="addr__value" title={value}>
        {value}
      </span>
      <button
        className={`addr__copy${copied ? " is-copied" : ""}`}
        type="button"
        aria-label={copied ? `Copied ${value}` : `Copy ${label} ${value}`}
        onClick={() => {
          navigator.clipboard.writeText(value).then(
            () => setCopied(true),
            () => setCopied(false),
          );
        }}
      >
        <Icon name={copied ? "check" : "copy"} size={12} />
      </button>
    </div>
  );
}

export function TunnelList({
  rows,
  totalCount,
  query,
  pending,
  leaving,
  selection,
  onSelect,
  onStart,
  onStop,
  onRestart,
  onRequestDelete,
  onEdit,
  onDuplicate,
  onShowLogs,
  onCreate,
  onImportSsh,
  onClearQuery,
}: {
  rows: TunnelRow[];
  totalCount: number;
  query: string;
  pending: Record<string, boolean>;
  leaving: ReadonlySet<string>;
  selection: ReadonlySet<string>;
  onSelect: (ids: string[], selected: boolean) => void;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onRestart: (id: string) => void;
  onRequestDelete: (row: TunnelRow) => void;
  onEdit: (row: TunnelRow) => void;
  onDuplicate: (row: TunnelRow) => void;
  onShowLogs: (row: TunnelRow) => void;
  onCreate: () => void;
  onImportSsh: () => void;
  onClearQuery: () => void;
}) {
  const [now, setNow] = useState(() => Date.now());
  const [menu, setMenu] = useState<{ id: string; anchor: HTMLElement } | null>(null);
  const hasLive = rows.some((r) => r.status.pid != null);
  const checkAllRef = useRef<HTMLInputElement>(null);
  const selectedVisible = rows.filter((r) => selection.has(r.id)).length;
  const allSelected = rows.length > 0 && selectedVisible === rows.length;
  const someSelected = selectedVisible > 0 && !allSelected;

  useEffect(() => {
    if (!hasLive) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [hasLive]);

  useEffect(() => {
    if (checkAllRef.current) checkAllRef.current.indeterminate = someSelected;
  }, [someSelected, rows.length]);

  if (rows.length === 0) {
    const onboarding = totalCount === 0;
    return (
      <div className="empty" role="status" aria-live="polite">
        <svg className="empty__art" viewBox="0 0 132 104" fill="none" aria-hidden="true">
          <rect x="14" y="18" width="104" height="68" rx="8" stroke="currentColor" strokeWidth="1.5" />
          <path d="M14 34h104" stroke="currentColor" strokeWidth="1.5" />
          <circle cx="25" cy="26" r="2.6" fill="currentColor" opacity="0.6" />
          <circle cx="34" cy="26" r="2.6" fill="currentColor" opacity="0.35" />
          <path d="M30 46h26M30 56h44M30 66h34" stroke="currentColor" strokeWidth="2" strokeLinecap="round" opacity="0.85" />
          <circle cx="96" cy="26" r="3" fill="var(--accent)" opacity="0.9" />
          <path d="M86 50h22M86 60h16M86 70h22" stroke="currentColor" strokeWidth="2" strokeLinecap="round" opacity="0.35" />
          <path d="M60 92v-8" stroke="currentColor" strokeWidth="1.5" strokeDasharray="2 3" opacity="0.5" />
        </svg>
        {onboarding ? (
          <>
            <h2 className="empty__title">No tunnels yet</h2>
            <p className="empty__desc">
              A tunnel is a saved, typed profile — not a shell one-liner you
              retype every morning. Start with a local forward like{" "}
              <span className="empty__cmd">127.0.0.1:5433 → db.internal:5432</span>{" "}
              through your bastion.
            </p>
            <div className="empty__actions">
              <button className="btn btn--primary" type="button" onClick={onCreate}>
                <Icon name="plus" size={15} />
                Create your first tunnel
              </button>
              <button className="btn btn--ghost" type="button" onClick={onImportSsh}>
                <Icon name="file-code" size={15} />
                Import from ~/.ssh/config
              </button>
            </div>
          </>
        ) : (
          <>
            <h2 className="empty__title">No tunnels match</h2>
            <p className="empty__desc">
              Nothing matches <span className="empty__cmd">{query.trim()}</span>.
              Search checks name, SSH target, remote host, and both ports.
            </p>
            <div className="empty__actions">
              <button className="btn btn--ghost" type="button" onClick={onClearQuery}>
                <Icon name="x" size={15} />
                Clear search
              </button>
            </div>
          </>
        )}
      </div>
    );
  }

  const menuRow = menu ? rows.find((r) => r.id === menu.id) : undefined;
  const menuEntries: MenuEntry[] = menuRow
    ? [
        {
          kind: "item",
          key: "restart",
          label: "Restart",
          icon: "restart",
          disabled: pending[menuRow.id] ?? false,
          onSelect: () => onRestart(menuRow.id),
        },
        {
          kind: "item",
          key: "edit",
          label: "Edit tunnel",
          icon: "edit",
          onSelect: () => onEdit(menuRow),
        },
        {
          kind: "item",
          key: "duplicate",
          label: "Duplicate",
          icon: "copy",
          onSelect: () => onDuplicate(menuRow),
        },
        {
          kind: "item",
          key: "logs",
          label: "View logs",
          icon: "terminal",
          shortcut: "L",
          onSelect: () => onShowLogs(menuRow),
        },
        { kind: "sep", key: "sep" },
        {
          kind: "item",
          key: "delete",
          label: "Delete tunnel",
          icon: "trash",
          danger: true,
          onSelect: () => onRequestDelete(menuRow),
        },
      ]
    : [];

  return (
    <>
      <table className="tunnels" aria-label="SSH tunnels">
        <colgroup>
          <col className="col-check" />
          <col className="col-name" />
          <col className="col-mode" />
          <col className="col-local" />
          <col className="col-remote" />
          <col className="col-status" />
          <col className="col-detail" />
          <col className="col-actions" />
        </colgroup>
        <thead>
          <tr>
            <th scope="col">
              <input
                ref={checkAllRef}
                type="checkbox"
                className="check"
                aria-label="Select all visible tunnels"
                checked={allSelected}
                onChange={(e) => onSelect(rows.map((r) => r.id), e.target.checked)}
              />
            </th>
            <th scope="col">Name</th>
            <th scope="col">Mode</th>
            <th scope="col">Local</th>
            <th scope="col">Remote</th>
            <th scope="col">Status</th>
            <th scope="col">Detail</th>
            <th scope="col">
              <span className="visually-hidden">Actions</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => {
            const busy = pending[row.id] ?? false;
            const state = row.status.state;
            const startable = canStart(state) && !busy;
            const stoppable = canStop(state) && !busy;
            const detail = detailText(row, now);
            const local = `${row.bindAddress}:${row.localPort}`;
            const selected = selection.has(row.id);
            const cls = [selected ? "is-selected" : "", leaving.has(row.id) ? "is-leaving" : ""]
              .filter(Boolean)
              .join(" ");
            return (
              <tr key={row.id} data-id={row.id} data-state={state} className={cls || undefined}>
                <td>
                  <input
                    type="checkbox"
                    className="check"
                    aria-label={`Select ${row.name}`}
                    checked={selected}
                    onChange={(e) => onSelect([row.id], e.target.checked)}
                  />
                </td>
                <td className="cell-name">
                  <div className="name-stack">
                    <span className="name-stack__title">{row.name}</span>
                    <span className="name-stack__sub">{row.sshTarget}</span>
                  </div>
                </td>
                <td>
                  <span className="mode-chip" title={`${MODE_LABEL[row.mode]} forward`}>
                    <span className="mode-chip__flag">{MODE_FLAG[row.mode]}</span>
                    {MODE_LABEL[row.mode]}
                  </span>
                </td>
                <td>
                  <AddressCell value={local} label="local address" />
                </td>
                <td>
                  <AddressCell
                    value={remoteLabel(row)}
                    label="remote address"
                    muted={row.mode === "dynamic"}
                  />
                </td>
                <td>
                  <StatusBadge state={state} restartCount={row.status.restartCount} />
                </td>
                <td>
                  <div
                    className={`detail-cell${detail.error ? " is-error" : ""}${detail.text === "—" ? " is-idle" : ""}`}
                    title={detail.text}
                  >
                    {detail.text}
                  </div>
                </td>
                <td>
                  <div className="actions-cell">
                    {startable && (
                      <button
                        className="act act--play"
                        type="button"
                        aria-label={`Start ${row.name}`}
                        title="Start"
                        onClick={() => onStart(row.id)}
                      >
                        <Icon name="play" size={13} />
                      </button>
                    )}
                    {stoppable && (
                      <button
                        className="act act--stop"
                        type="button"
                        aria-label={`Stop ${row.name}`}
                        title="Stop"
                        onClick={() => onStop(row.id)}
                      >
                        <Icon name="stop" size={13} />
                      </button>
                    )}
                    {busy && (
                      <button className="act act--busy" type="button" disabled aria-label="Working">
                        <Icon name="loader" size={13} className="spinner" />
                      </button>
                    )}
                    <button
                      className="act"
                      type="button"
                      aria-label={`More actions for ${row.name}`}
                      aria-haspopup="menu"
                      aria-expanded={menu?.id === row.id}
                      title="More actions"
                      onClick={(e) => {
                        const anchor = e.currentTarget;
                        setMenu((m) => (m?.id === row.id ? null : { id: row.id, anchor }));
                      }}
                    >
                      <Icon name="more" size={14} />
                    </button>
                  </div>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>

      {menu && menuRow && (
        <Menu
          anchor={menu.anchor}
          entries={menuEntries}
          label={`Actions for ${menuRow.name}`}
          onClose={() => setMenu(null)}
        />
      )}
    </>
  );
}
