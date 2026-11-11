import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ConfirmModal, type ConfirmContent } from "./components/ConfirmModal";
import { LogDrawer } from "./components/LogDrawer";
import type { MenuEntry } from "./components/Menu";
import { Toasts, useToasts } from "./components/Toasts";
import { Topbar } from "./components/Topbar";
import { TunnelForm } from "./components/TunnelForm";
import { TunnelList } from "./components/TunnelList";
import {
  createTunnel,
  deleteTunnel,
  exportTunnelsJson,
  getRuntimeStatus,
  importSshConfig,
  importTunnelsJson,
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
import { applyTheme, type Theme } from "./lib/theme";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import type {
  TunnelConfig,
  TunnelInput,
  TunnelRow,
  TunnelStatus,
} from "./lib/types";

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

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export default function App() {
  const [rows, setRows] = useState<TunnelRow[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
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
  const [theme, setTheme] = useState<Theme>(
    () => (document.documentElement.dataset.theme as Theme) ?? "dark",
  );
  const [deleteTargets, setDeleteTargets] = useState<TunnelRow[] | null>(null);
  const [leaving, setLeaving] = useState<ReadonlySet<string>>(() => new Set());
  const { items: toasts, push: toast } = useToasts();
  const prevStates = useRef<Record<string, string>>({});
  const searchRef = useRef<HTMLInputElement>(null);

  const reload = useCallback(() => refresh(setRows, setLoadError), []);

  useEffect(() => {
    reload();
    void ensureNotificationPermission();
    let unsubscribe: (() => void) | undefined;
    onTunnelEvent((event) => {
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
      reload();
    })
      .then((fn) => {
        unsubscribe = fn;
      })
      .catch(() => {
        // Outside the Tauri runtime there is no event bus.
      });
    return () => unsubscribe?.();
  }, [reload]);

  useEffect(() => {
    isEnabled()
      .then(setAutostart)
      .catch(() => setAutostart(null));
  }, []);

  const openNew = useCallback(() => {
    setDraft(null);
    setSaveError(null);
    setFormTarget(null);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const inField = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
      if (e.key === "/" && !inField) {
        e.preventDefault();
        searchRef.current?.focus();
      } else if (e.key === "Escape") {
        // An open <dialog> handles Esc itself through its cancel event.
        if (document.querySelector("dialog[open]")) return;
        if (formTarget !== undefined || logTarget) {
          setDraft(null);
          setFormTarget(undefined);
          setLogTarget(null);
        } else if (query) {
          setQuery("");
        }
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        openNew();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [query, formTarget, logTarget, openNew]);

  const runAction = useCallback(
    async (id: string, action: (id: string) => Promise<unknown>) => {
      setPending((p) => ({ ...p, [id]: true }));
      try {
        await action(id);
      } catch (e) {
        toast(String(e), "error");
      } finally {
        setPending((p) => ({ ...p, [id]: false }));
        reload();
      }
    },
    [reload, toast],
  );

  const stats = useMemo(
    () => ({
      running: rows.filter(
        (r) => r.status.state === "starting" || r.status.state === "running",
      ).length,
      failed: rows.filter((r) => r.status.state === "failed").length,
    }),
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
    async (input: TunnelInput): Promise<TunnelConfig | void> => {
      setSaving(true);
      setSaveError(null);
      try {
        let saved: TunnelConfig | void;
        if (formTarget?.id) {
          saved = await updateTunnel(formTarget.id, input);
        } else {
          saved = await createTunnel(input);
        }
        setDraft(null);
        setFormTarget(undefined);
        reload();
        return saved;
      } catch (e) {
        setSaveError(String(e));
        throw e;
      } finally {
        setSaving(false);
      }
    },
    [formTarget, reload],
  );

  const handleDuplicate = useCallback((row: TunnelRow) => {
    setSaveError(null);
    setDraft({
      name: `${row.name} (copy)`,
      mode: row.mode,
      sshTarget: row.sshTarget,
      bindAddress: row.bindAddress,
      localPort: row.localPort,
      remoteHost: row.remoteHost,
      remotePort: row.remotePort,
      identityFile: row.identityFile,
      proxyJump: row.proxyJump,
      autoStart: false,
      keepAliveInterval: row.keepAliveInterval,
      maxRetries: row.maxRetries,
      retryDelaySecs: row.retryDelaySecs,
    });
    setFormTarget(null);
  }, []);

  const handleExportJson = useCallback(async () => {
    try {
      const json = await exportTunnelsJson();
      await navigator.clipboard.writeText(json);
      toast(`${plural(rows.length, "tunnel")} copied to clipboard as JSON`, "success");
    } catch (e) {
      toast(`Export failed: ${String(e)}`, "error");
    }
  }, [rows.length, toast]);

  const handleImportJson = useCallback(async () => {
    const json = prompt("Paste tunnels JSON (array of TunnelInput):");
    if (!json) return;
    try {
      const created = await importTunnelsJson(json);
      toast(`Imported ${plural(created.length, "tunnel")} from JSON`, "success");
      reload();
    } catch (e) {
      toast(`Import failed: ${String(e)}`, "error");
    }
  }, [reload, toast]);

  const handleImportSsh = useCallback(async () => {
    try {
      const created = await importSshConfig(null);
      toast(
        created.length === 0
          ? "No LocalForward entries found in ~/.ssh/config"
          : `Imported ${plural(created.length, "tunnel")} from ~/.ssh/config`,
        created.length === 0 ? "info" : "success",
      );
      reload();
    } catch (e) {
      toast(`SSH config import failed: ${String(e)}`, "error");
    }
  }, [reload, toast]);

  const toggleAutostart = useCallback(async () => {
    if (autostart === null) return;
    const next = !autostart;
    setAutostart(next);
    try {
      if (next) await enable();
      else await disable();
      toast(next ? "tnls will start on login" : "tnls will not start on login", "info");
    } catch (e) {
      toast(`Could not change start-on-login: ${String(e)}`, "error");
      setAutostart(await isEnabled().catch(() => null));
    }
  }, [autostart, toast]);

  const handleBulk = useCallback(
    async (action: (id: string) => Promise<unknown>) => {
      const ids = visibleRows.map((r) => r.id);
      for (const id of ids) {
        setPending((p) => ({ ...p, [id]: true }));
      }
      for (const id of ids) {
        try {
          await action(id);
        } catch (e) {
          toast(String(e), "error");
        }
      }
      for (const id of ids) {
        setPending((p) => ({ ...p, [id]: false }));
      }
      reload();
    },
    [visibleRows, reload, toast],
  );

  const deleteContent = useMemo<ConfirmContent | null>(() => {
    if (!deleteTargets || deleteTargets.length === 0) return null;
    const one = deleteTargets.length === 1;
    return {
      title: one ? "Delete tunnel?" : `Delete ${deleteTargets.length} tunnels?`,
      text:
        (one ? "This removes the saved profile" : "This removes the saved profiles") +
        " and any passphrase stored in the OS keychain. A running process is stopped first. This cannot be undone.",
      target: one
        ? `${deleteTargets[0].name} — ${deleteTargets[0].sshTarget}`
        : deleteTargets.map((r) => r.name).join(", "),
      confirmLabel: one ? "Delete tunnel" : `Delete ${deleteTargets.length} tunnels`,
    };
  }, [deleteTargets]);

  const confirmDelete = useCallback(() => {
    const targets = deleteTargets ?? [];
    setDeleteTargets(null);
    if (targets.length === 0) return;
    const ids = targets.map((r) => r.id);
    // Let the rows animate out before they disappear from the data.
    setLeaving((cur) => new Set([...cur, ...ids]));
    setTimeout(async () => {
      let removed = 0;
      for (const row of targets) {
        try {
          await deleteTunnel(row.id);
          removed += 1;
        } catch (e) {
          toast(`Could not delete "${row.name}": ${String(e)}`, "error");
        }
      }
      setLogTarget((cur) => (cur && ids.includes(cur.id) ? null : cur));
      await reload();
      setLeaving((cur) => {
        const next = new Set(cur);
        ids.forEach((id) => next.delete(id));
        return next;
      });
      if (removed > 0) {
        toast(
          removed === 1 && targets.length === 1
            ? `Deleted "${targets[0].name}"`
            : `Deleted ${plural(removed, "tunnel")}`,
          "success",
        );
      }
    }, 180);
  }, [deleteTargets, reload, toast]);

  const moreEntries: MenuEntry[] = [
    { kind: "label", key: "import-label", label: "Import" },
    {
      kind: "item",
      key: "import-ssh",
      label: "From ~/.ssh/config",
      icon: "file-code",
      onSelect: handleImportSsh,
    },
    {
      kind: "item",
      key: "import-json",
      label: "From JSON",
      icon: "upload",
      onSelect: handleImportJson,
    },
    { kind: "sep", key: "sep-1" },
    { kind: "label", key: "export-label", label: "Export" },
    {
      kind: "item",
      key: "export",
      label: "Copy all as JSON",
      icon: "download",
      disabled: rows.length === 0,
      onSelect: handleExportJson,
    },
    { kind: "sep", key: "sep-2" },
    ...(autostart !== null
      ? [
          {
            kind: "item" as const,
            key: "autostart",
            label: "Start tnls on login",
            icon: autostart ? ("check" as const) : ("circle" as const),
            shortcut: autostart ? "on" : "off",
            onSelect: toggleAutostart,
          },
        ]
      : []),
    {
      kind: "item",
      key: "refresh",
      label: "Reload from backend",
      icon: "refresh",
      onSelect: reload,
    },
  ];

  const trimmedQuery = query.trim();

  return (
    <>
      <div className="app">
        <Topbar
          total={rows.length}
          running={stats.running}
          failed={stats.failed}
          query={query}
          onQuery={setQuery}
          searchRef={searchRef}
          theme={theme}
          onToggleTheme={() => {
            const next: Theme = theme === "dark" ? "light" : "dark";
            applyTheme(next);
            setTheme(next);
          }}
          moreEntries={moreEntries}
          onNew={openNew}
        />

        <main className="main">
          <div className="table-toolbar">
            <span className="results-note" aria-live="polite">
              {trimmedQuery !== "" &&
                `${visibleRows.length} of ${rows.length} match "${trimmedQuery}"`}
            </span>
            {rows.length > 1 && (
              <>
                <button className="btn btn--sm btn--ghost" onClick={() => handleBulk(startTunnel)}>
                  Start all visible
                </button>
                <button className="btn btn--sm btn--ghost" onClick={() => handleBulk(stopTunnel)}>
                  Stop all visible
                </button>
              </>
            )}
            <div className="toolbar__spacer" />
            <div className="toolbar__hint">
              <kbd>/</kbd>
              <span>search</span>
              <kbd>Ctrl</kbd>
              <kbd>N</kbd>
              <span>new</span>
              <kbd>Esc</kbd>
              <span>close</span>
            </div>
          </div>

          <div className="table-scroll">
            {loadError && (
              <p
                className="form-error"
                role="alert"
                style={{ margin: "var(--s-3) var(--page-pad)", alignItems: "center" }}
              >
                <span style={{ flex: "1 1 auto" }}>{loadError}</span>
                <button className="btn btn--sm btn--ghost" type="button" onClick={reload}>
                  Retry
                </button>
              </p>
            )}
            <TunnelList
              rows={visibleRows}
              totalCount={rows.length}
              query={query}
              pending={pending}
              leaving={leaving}
              onStart={(id) => runAction(id, startTunnel)}
              onStop={(id) => runAction(id, stopTunnel)}
              onRestart={(id) => runAction(id, restartTunnel)}
              onRequestDelete={(row) => setDeleteTargets([row])}
              onEdit={(row) => {
                setDraft(null);
                setSaveError(null);
                setFormTarget(row);
              }}
              onDuplicate={handleDuplicate}
              onShowLogs={setLogTarget}
              onCreate={openNew}
              onImportSsh={handleImportSsh}
              onClearQuery={() => {
                setQuery("");
                searchRef.current?.focus();
              }}
            />
          </div>
        </main>
      </div>

      <ConfirmModal
        content={deleteContent}
        onCancel={() => setDeleteTargets(null)}
        onConfirm={confirmDelete}
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
          onClose={() => {
            setDraft(null);
            setFormTarget(undefined);
          }}
        />
      )}

      <Toasts items={toasts} />
    </>
  );
}
