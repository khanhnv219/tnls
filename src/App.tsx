import { useCallback, useEffect, useMemo, useState } from "react";
import { LogDrawer } from "./components/LogDrawer";
import { TunnelList } from "./components/TunnelList";
import {
  deleteTunnel,
  getRuntimeStatus,
  listTunnels,
  onTunnelEvent,
  restartTunnel,
  startTunnel,
  stopTunnel,
} from "./lib/api";
import type { TunnelConfig, TunnelRow, TunnelStatus } from "./lib/types";
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

  useEffect(() => {
    refresh(setRows, setLoadError);
    let unsubscribe: (() => void) | undefined;
    // Any lifecycle event invalidates the status cache: refetch.
    // Cheap (local IPC) and always consistent with the supervisor.
    onTunnelEvent(() => refresh(setRows, setLoadError)).then((fn) => {
      unsubscribe = fn;
    });
    return () => unsubscribe?.();
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
        <button
          className="ghost"
          onClick={() => refresh(setRows, setLoadError)}
          title="Reload from backend"
        >
          Refresh
        </button>
      </header>

      {loadError && (
        <p className="error" role="alert">
          {loadError}{" "}
          <button className="ghost" onClick={() => refresh(setRows, setLoadError)}>
            Retry
          </button>
        </p>
      )}

      <TunnelList
        rows={rows}
        pending={pending}
        actionError={actionError}
        onStart={(id) => runAction(id, startTunnel)}
        onStop={(id) => runAction(id, stopTunnel)}
        onRestart={(id) => runAction(id, restartTunnel)}
        onDelete={(id) => runAction(id, deleteTunnel)}
        onShowLogs={setLogTarget}
      />

      {logTarget && (
        <LogDrawer
          tunnelId={logTarget.id}
          tunnelName={logTarget.name}
          onClose={() => setLogTarget(null)}
        />
      )}
    </main>
  );
}
