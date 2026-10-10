import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LogDrawer } from "./components/LogDrawer";
import { TunnelForm } from "./components/TunnelForm";
import { TunnelList } from "./components/TunnelList";
import {
  createTunnel,
  deleteTunnel,
  getRuntimeStatus,
  listTunnels,
  onTunnelEvent,
  restartTunnel,
  startTunnel,
  stopTunnel,
  updateTunnel,
} from "./lib/api";
import {
  ensureNotificationPermission,
  fireNotification,
  notifyForEvent,
} from "./lib/notify";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import type {
  TunnelConfig,
  TunnelInput,
  TunnelRow,
  TunnelStatus,
} from "./lib/types";
import "./App.css";

function mergeRows(
  configs: TunnelConfig[],
  statuses: TunnelStatus[],
): TunnelRow[] {
  const byId: Record<string, TunnelStatus> = {};
  for (const s of statuses) byId[s.id] = s;
  return configs.map((c) => ({
    ...c,
    status: byId[c.id] ?? {
      id: c.id,
      state: "stopped",
      pid: null,
      startedAt: null,
      lastError: null,
      restartCount: 0,
    },
  }));
}

async function refresh(
  setRows: (rows: TunnelRow[]) => void,
  setLoadError: (msg: string | null) => void,
) {
  try {
    const [configs, statuses] = await Promise.all([
      listTunnels(),
      getRuntimeStatus(),
    ]);
    setRows(mergeRows(configs, statuses));
    setLoadError(null);
  } catch (e) {
    setLoadError(`Failed to load tunnels: ${String(e)}`);
  }
}
export default function App() {
  const [rows, setRows] = useState<TunnelRow[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [pending, setPending] = useState<Record<string, boolean>>({});
  const [logTarget, setLogTarget] = useState<TunnelRow | null>(null);
  const [formTarget, setFormTarget] = useState<TunnelConfig | null | undefined>(
    undefined,
  );
  const [draft, setDraft] = useState<TunnelInput | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const prevStates = useRef<Record<string, string>>({});

  useEffect(() => {
    refresh(setRows, setLoadError);
    void ensureNotificationPermission();
    let unsubscribe: (() => void) | undefined;
    onTunnelEvent((event) => {
      // Notify before refetch (lookup needs current rows).
      setRows((current) => {
        const lookup = (id: string) =>
          current.find((r) => r.id === id)?.name ?? id;
        const note = notifyForEvent(event, lookup, prevStates.current[event.id]);
        if (note) fireNotification(note.title, note.body);
        if (event.kind.type === "stateChanged") {
          prevStates.current[event.id] = event.kind.state;
        }
        return current;
      });
      refresh(setRows, setLoadError);
    }).then((fn) => {
      unsubscribe = fn;
    });
    return () => unsubscribe?.();
  }, []);

  useEffect(() => {
    isEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);

  const runAction = useCallback(
    async (id: string, action: (id: string) => Promise<unknown>) => {
      setPending((p) => ({ ...p, [id]: true }));
      setActionError(null);
      try {
        await action(id);
      } catch (e) {
        setActionError(String(e));
      } finally {
        setPending((p) => ({ ...p, [id]: false }));
        refresh(setRows, setLoadError);
      }
    },
    [],
  );

  const runningCount = useMemo(
    () =>
      rows.filter(
        (r) => r.status.state === "starting" || r.status.state === "running",
      ).length,
    [rows],
  );

  const visibleRows = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return rows;
    return rows.filter((r) =>
      [r.name, r.sshTarget, r.remoteHost ?? "", `${r.localPort}`, `${r.remotePort ?? ""}`]
        .join(" ")
        .toLowerCase()
        .includes(q),
    );
  }, [rows, query]);

  const handleSave = useCallback(
    async (input: TunnelInput) => {
      setSaving(true);
      setSaveError(null);
      try {
        if (formTarget?.id) {
          await updateTunnel(formTarget.id, input);
        } else {
          await createTunnel(input);
        }
        setDraft(null);
        setFormTarget(undefined);
        refresh(setRows, setLoadError);
      } catch (e) {
        setSaveError(String(e));
      } finally {
        setSaving(false);
      }
    },
    [formTarget],
  );

  const handleDuplicate = useCallback((row: TunnelRow) => {
    setSaveError(null);
    // Prefill a *creation* form from an existing row: same settings,
    // new name; handleSave creates because formTarget stays null.
    setDraft({
      name: `${row.name} (copy)`,
      mode: row.mode,
      sshTarget: row.sshTarget,
      bindAddress: row.bindAddress,
      localPort: row.localPort,
      remoteHost: row.remoteHost,
      remotePort: row.remotePort,
      identityFile: row.identityFile,
      autoStart: false,
      keepAliveInterval: row.keepAliveInterval,
    });
    setFormTarget(null);
  }, []);
  return (
    <main className="app">
      <header className="topbar">
        <div>
          <h1>tnls</h1>
          <p className="muted">
            {rows.length} tunnel{rows.length === 1 ? "" : "s"} · {runningCount}{" "}
            running
          </p>
        </div>
        <div className="top-actions">
          <input
            className="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search name, host, port…"
            aria-label="Search tunnels"
          />
          <button
            className="ghost"
            onClick={() => refresh(setRows, setLoadError)}
            title="Reload from backend"
          >
            Refresh
          </button>
          <button onClick={() => { setDraft(null); setSaveError(null); setFormTarget(null); }}>
            + New tunnel
          </button>
        </div>
      </header>

      {autostart !== null && (
        <label className="autostart-row">
          <input
            type="checkbox"
            checked={autostart}
            onChange={async (e) => {
              const next = e.target.checked;
              setAutostart(next);
              try {
                if (next) await enable();
                else await disable();
              } catch {
                try {
                  setAutostart(await isEnabled());
                } catch {
                  setAutostart(null);
                }
              }
            }}
          />
          Start on login
        </label>
      )}

      {loadError && (
        <p className="error" role="alert">
          {loadError}{" "}
          <button className="ghost" onClick={() => refresh(setRows, setLoadError)}>
            Retry
          </button>
        </p>
      )}

      {query.trim() !== "" && (
        <p className="muted">
          {visibleRows.length} of {rows.length} shown
        </p>
      )}

      <TunnelList
        rows={visibleRows}
        pending={pending}
        actionError={actionError}
        onStart={(id) => runAction(id, startTunnel)}
        onStop={(id) => runAction(id, stopTunnel)}
        onRestart={(id) => runAction(id, restartTunnel)}
        onDelete={(id) => runAction(id, deleteTunnel)}
        onEdit={(row) => { setDraft(null); setSaveError(null); setFormTarget(row); }}
        onDuplicate={handleDuplicate}
        onShowLogs={setLogTarget}
      />

      {logTarget && (
        <LogDrawer
          tunnelId={logTarget.id}
          tunnelName={logTarget.name}
          onClose={() => setLogTarget(null)}
        />
      )}

      {formTarget !== undefined && (
        <TunnelForm
          key={formTarget?.id ?? `new-${draft?.name ?? "blank"}`}
          initial={formTarget}
          draft={draft}
          saving={saving}
          saveError={saveError}
          onSave={handleSave}
          onClose={() => { setDraft(null); setFormTarget(undefined); }}
        />
      )}
    </main>
  );
}
