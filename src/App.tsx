import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BulkBar } from "./components/BulkBar";
import { ConfirmModal, type ConfirmContent } from "./components/ConfirmModal";
import { Drawer } from "./components/Drawer";
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
  setPassphrase,
  startTunnel,
  stopTunnel,
  updateTunnel,
} from "./lib/api";
import {
  ensureNotificationPermission,
  fireNotification,
  notifyForEvent,
} from "./lib/notify";
import { canStart, canStop } from "./lib/states";
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
  const [selection, setSelection] = useState<ReadonlySet<string>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
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

  const closeForm = useCallback(() => {
    setDraft(null);
    setFormTarget(undefined);
  }, []);

  const closeLogs = useCallback(() => setLogTarget(null), []);

  // Keyboard map (DESIGN_SYSTEM.md §6.11). Menus handle their own Esc in the
  // capture phase; an open <dialog> handles Esc through its cancel event.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const inField = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
      const dialogOpen = document.querySelector("dialog[open]") !== null;
      const formOpen = formTarget !== undefined;
      const logOpen = logTarget !== null;

      if (e.key === "Escape") {
        if (dialogOpen) return;
        if (formOpen) closeForm();
        else if (logOpen) closeLogs();
        else if (selection.size > 0) setSelection(new Set());
        else if (query) setQuery("");
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
        e.preventDefault();
        if (!formOpen && !dialogOpen) openNew();
        return;
      }
      if (inField || dialogOpen || e.ctrlKey || e.metaKey || e.altKey) return;

      if (e.key === "/") {
        e.preventDefault();
        if (logOpen) {
          document.querySelector<HTMLInputElement>(".logbar__search input")?.focus();
        } else if (!formOpen) {
          searchRef.current?.focus();
        }
      } else if (e.key.toLowerCase() === "l" && !formOpen) {
        // Toggle the log drawer for the row that currently holds focus.
        if (logOpen) {
          e.preventDefault();
          closeLogs();
          return;
        }
        const id = target.closest<HTMLElement>("tr[data-id]")?.dataset.id;
        const row = id ? rows.find((r) => r.id === id) : undefined;
        if (row) {
          e.preventDefault();
          setLogTarget(row);
        }
      } else if (e.key === "Delete" && selection.size > 0 && !formOpen && !logOpen) {
        e.preventDefault();
        setDeleteTargets(rows.filter((r) => selection.has(r.id)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [query, formTarget, logTarget, openNew, closeForm, closeLogs, selection, rows]);

  // Drop selected ids whose rows no longer exist (deleted elsewhere).
  useEffect(() => {
    setSelection((cur) => {
      if (cur.size === 0) return cur;
      const ids = new Set(rows.map((r) => r.id));
      const next = new Set([...cur].filter((id) => ids.has(id)));
      return next.size === cur.size ? cur : next;
    });
  }, [rows]);

  const selectRows = useCallback((ids: string[], selected: boolean) => {
    setSelection((cur) => {
      const next = new Set(cur);
      ids.forEach((id) => (selected ? next.add(id) : next.delete(id)));
      return next;
    });
  }, []);

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
    async (input: TunnelInput, passphrase: string): Promise<void> => {
      setSaving(true);
      setSaveError(null);
      const editing = formTarget?.id ? formTarget : null;
      let saved: TunnelConfig;
      try {
        saved = editing
          ? await updateTunnel(editing.id, input)
          : await createTunnel(input);
      } catch (e) {
        setSaveError(String(e));
        throw e;
      } finally {
        setSaving(false);
      }
      setDraft(null);
      setFormTarget(undefined);
      toast(
        editing ? `Saved changes to "${saved.name}"` : `Created tunnel "${saved.name}"`,
        "success",
      );
      if (passphrase.trim() !== "") {
        try {
          await setPassphrase(saved.id, passphrase);
          toast("Passphrase saved to OS keychain", "success");
        } catch (e) {
          toast(`Keychain save failed: ${String(e)}`, "error");
        }
      }
      reload();
    },
    [formTarget, reload, toast],
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

  /** Run one action over the selected rows that are in a legal state for it. */
  const handleBulk = useCallback(
    async (kind: "start" | "stop" | "restart") => {
      const eligible = rows.filter(
        (r) =>
          selection.has(r.id) &&
          (kind === "start"
            ? canStart(r.status.state)
            : kind === "stop"
              ? canStop(r.status.state)
              : true),
      );
      if (eligible.length === 0) {
        toast(
          kind === "start"
            ? "All selected tunnels are already up"
            : kind === "stop"
              ? "None of the selected tunnels are running"
              : "Nothing to restart",
          "info",
        );
        return;
      }
      const action =
        kind === "start" ? startTunnel : kind === "stop" ? stopTunnel : restartTunnel;
      const ids = eligible.map((r) => r.id);
      setBulkBusy(true);
      setPending((p) => ({ ...p, ...Object.fromEntries(ids.map((id) => [id, true])) }));
      let failures = 0;
      for (const row of eligible) {
        try {
          await action(row.id);
        } catch (e) {
          failures += 1;
          toast(`${row.name}: ${String(e)}`, "error");
        }
      }
      setPending((p) => ({ ...p, ...Object.fromEntries(ids.map((id) => [id, false])) }));
      setBulkBusy(false);
      const verb = kind === "start" ? "Starting" : kind === "stop" ? "Stopping" : "Restarting";
      const done = eligible.length - failures;
      if (done > 0) toast(`${verb} ${plural(done, "tunnel")}`, "info");
      reload();
    },
    [rows, selection, reload, toast],
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

        <main className={`main${selection.size > 0 ? " has-bulk" : ""}`}>
          <div className="table-toolbar">
            <span className="results-note" aria-live="polite">
              {trimmedQuery !== "" &&
                `${visibleRows.length} of ${rows.length} match "${trimmedQuery}"`}
            </span>
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
              selection={selection}
              onSelect={selectRows}
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

          <BulkBar
            count={selection.size}
            busy={bulkBusy}
            onStart={() => handleBulk("start")}
            onStop={() => handleBulk("stop")}
            onRestart={() => handleBulk("restart")}
            onDelete={() => setDeleteTargets(rows.filter((r) => selection.has(r.id)))}
            onClear={() => setSelection(new Set())}
          />
        </main>
      </div>

      <ConfirmModal
        content={deleteContent}
        onCancel={() => setDeleteTargets(null)}
        onConfirm={confirmDelete}
      />

      <Drawer open={logTarget !== null} wide labelledBy="log-title" onClose={closeLogs}>
        {logTarget && (
          <LogDrawer
            key={logTarget.id}
            tunnelId={logTarget.id}
            tunnelName={logTarget.name}
            onClose={closeLogs}
            notify={toast}
          />
        )}
      </Drawer>

      <Drawer open={formTarget !== undefined} labelledBy="form-title" onClose={closeForm}>
        {formTarget !== undefined && (
          <TunnelForm
            key={formTarget?.id ?? `new-${draft?.name ?? "blank"}`}
            initial={formTarget}
            draft={draft}
            saving={saving}
            saveError={saveError}
            onSave={handleSave}
            onClose={closeForm}
            notify={toast}
          />
        )}
      </Drawer>

      <Toasts items={toasts} />
    </>
  );
}
