import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { BulkBar } from "./components/BulkBar";
import { CloseAppModal } from "./components/CloseAppModal";
import { ConfirmModal, type ConfirmContent } from "./components/ConfirmModal";
import { Drawer } from "./components/Drawer";
import { Icon } from "./components/Icon";
import { ImportJsonModal } from "./components/ImportJsonModal";
import { PortConflictModal, type ConflictView } from "./components/PortConflictModal";
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
  PortConflictError,
  restartTunnel,
  takeOverPort,
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
import {
  hideToTray,
  onCloseRequested,
  quitApp,
  saveCloseAction,
  savedCloseAction,
  takeTrayHint,
  type CloseAction,
} from "./lib/closeAction";
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

/** Tag-filter value for tunnels without tags (not a valid tag: tags are trimmed). */
const UNGROUPED = " ungrouped";

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
  const [conflict, setConflict] = useState<(ConflictView & { tunnelId: string }) | null>(null);
  const [formFocus, setFormFocus] = useState<"name" | "localPort">("name");
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [leaving, setLeaving] = useState<ReadonlySet<string>>(() => new Set());
  const [selection, setSelection] = useState<ReadonlySet<string>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [closeOpen, setCloseOpen] = useState(false);
  const [closeAction, setCloseAction] = useState<CloseAction>(savedCloseAction);
  const closeActionRef = useRef(closeAction);
  closeActionRef.current = closeAction;
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

  const minimizeToTray = useCallback(async () => {
    try {
      await hideToTray();
      if (takeTrayHint()) {
        fireNotification(
          "tnls is still running",
          "Tunnels stay up. Click the tray icon to reopen the window.",
        );
      }
    } catch (e) {
      toast(`Could not minimize to tray: ${String(e)}`, "error");
    }
  }, [toast]);

  const quit = useCallback(async () => {
    try {
      await quitApp();
    } catch (e) {
      toast(`Could not quit: ${String(e)}`, "error");
    }
  }, [toast]);

  // The backend turns the window close button into this event.
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    onCloseRequested(() => {
      const action = closeActionRef.current;
      if (action === "tray") void minimizeToTray();
      else if (action === "quit") void quit();
      else setCloseOpen(true);
    })
      .then((fn) => {
        unsubscribe = fn;
      })
      .catch(() => {
        // Outside the Tauri runtime there is no window to close.
      });
    return () => unsubscribe?.();
  }, [minimizeToTray, quit]);

  const chooseClose = useCallback(
    (action: Exclude<CloseAction, "ask">, remember: boolean) => {
      setCloseOpen(false);
      if (remember) {
        saveCloseAction(action);
        setCloseAction(action);
      }
      if (action === "tray") void minimizeToTray();
      else void quit();
    },
    [minimizeToTray, quit],
  );

  const cycleCloseAction = useCallback(() => {
    const next: CloseAction =
      closeAction === "ask" ? "tray" : closeAction === "tray" ? "quit" : "ask";
    saveCloseAction(next);
    setCloseAction(next);
    toast(
      next === "ask"
        ? "Close button will ask what to do"
        : next === "tray"
          ? "Close button will minimize tnls to the tray"
          : "Close button will quit tnls and stop all tunnels",
      "info",
    );
  }, [closeAction, toast]);

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

  /** Start or restart one tunnel; a port conflict opens its dialog. */
  const startOne = useCallback(
    async (id: string, action: (id: string) => Promise<unknown>) => {
      setPending((p) => ({ ...p, [id]: true }));
      try {
        await action(id);
      } catch (e) {
        if (e instanceof PortConflictError) {
          const name = rows.find((r) => r.id === id)?.name ?? id;
          setConflict({ conflict: e.conflict, tunnelName: name, tunnelId: id });
        } else {
          toast(String(e), "error");
        }
      } finally {
        setPending((p) => ({ ...p, [id]: false }));
        reload();
      }
    },
    [rows, reload, toast],
  );

  const takeOver = useCallback(
    async (holderId: string) => {
      if (!conflict) return;
      const { tunnelId, tunnelName } = conflict;
      try {
        await takeOverPort(holderId, tunnelId);
        setConflict(null);
        toast(`Started "${tunnelName}" on ${conflict.conflict.bind}:${conflict.conflict.port}`, "success");
      } catch (e) {
        if (e instanceof PortConflictError) {
          // The other tunnel stopped, but something else still holds the port.
          setConflict({ conflict: e.conflict, tunnelName, tunnelId });
        } else {
          setConflict(null);
          toast(String(e), "error");
        }
      } finally {
        reload();
      }
    },
    [conflict, reload, toast],
  );

  const changePortFor = useCallback(() => {
    const row = conflict ? rows.find((r) => r.id === conflict.tunnelId) : undefined;
    setConflict(null);
    if (!row) return;
    setDraft(null);
    setSaveError(null);
    setFormFocus("localPort");
    setFormTarget(row);
  }, [conflict, rows]);

  const stats = useMemo(
    () => ({
      running: rows.filter(
        (r) => r.status.state === "starting" || r.status.state === "running",
      ).length,
      failed: rows.filter((r) => r.status.state === "failed").length,
    }),
    [rows],
  );

  // Tag filter: null = all tunnels, UNGROUPED = tunnels without tags.
  const tagGroups = useMemo(() => {
    const counts = new Map<string, number>();
    let ungrouped = 0;
    for (const r of rows) {
      if (r.tags.length === 0) ungrouped += 1;
      for (const t of r.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
    }
    const tags = [...counts.entries()].sort(([a], [b]) => a.localeCompare(b));
    return { tags, ungrouped };
  }, [rows]);

  // A filter on a tag that no longer exists falls back to "all".
  const activeTag =
    tagFilter === null ||
    tagFilter === UNGROUPED ||
    tagGroups.tags.some(([t]) => t === tagFilter)
      ? tagFilter
      : null;

  const visibleRows = useMemo(() => {
    const q = query.trim().toLowerCase();
    return rows.filter((r) => {
      if (activeTag === UNGROUPED && r.tags.length > 0) return false;
      if (activeTag !== null && activeTag !== UNGROUPED && !r.tags.includes(activeTag)) {
        return false;
      }
      if (q === "") return true;
      return [
        r.name,
        r.sshTarget,
        r.remoteHost ?? "",
        `${r.localPort}`,
        `${r.remotePort ?? ""}`,
        ...r.tags,
      ]
        .join(" ")
        .toLowerCase()
        .includes(q);
    });
  }, [rows, query, activeTag]);

  // Rows in the active tag group, ignoring the text search.
  const groupRows = useMemo(() => {
    if (activeTag === null) return [];
    return rows.filter((r) =>
      activeTag === UNGROUPED ? r.tags.length === 0 : r.tags.includes(activeTag),
    );
  }, [rows, activeTag]);

  const tagSuggestions = useMemo(() => tagGroups.tags.map(([t]) => t), [tagGroups]);

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
      tags: row.tags,
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

  const handleImportJson = useCallback(
    async (json: string) => {
      // Errors propagate to the dialog, which shows them next to the input.
      const created = await importTunnelsJson(json);
      setImportOpen(false);
      toast(`Imported ${plural(created.length, "tunnel")} from JSON`, "success");
      reload();
    },
    [reload, toast],
  );

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

  /**
   * Run one action over `targets` (the selection or a tag group), touching
   * only rows in a legal state for it.
   */
  const handleBulk = useCallback(
    async (kind: "start" | "stop" | "restart", targets: TunnelRow[]) => {
      const eligible = targets.filter((r) =>
        kind === "start"
          ? canStart(r.status.state)
          : kind === "stop"
            ? canStop(r.status.state)
            : true,
      );
      if (eligible.length === 0) {
        toast(
          kind === "start"
            ? "These tunnels are already up"
            : kind === "stop"
              ? "None of these tunnels are running"
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
    [reload, toast],
  );

  const selectedRows = useMemo(
    () => rows.filter((r) => selection.has(r.id)),
    [rows, selection],
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
      onSelect: () => setImportOpen(true),
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
      key: "close-action",
      label: "Close button",
      icon: "layers",
      shortcut: closeAction === "ask" ? "ask" : closeAction === "tray" ? "to tray" : "quit",
      onSelect: cycleCloseAction,
    },
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
            {tagGroups.tags.length > 0 && (
              <div className="tag-filter" role="group" aria-label="Filter by tag">
                <button
                  className="tag-filter__opt"
                  type="button"
                  aria-pressed={activeTag === null}
                  onClick={() => setTagFilter(null)}
                >
                  All <span className="tag-filter__count">{rows.length}</span>
                </button>
                {tagGroups.tags.map(([tag, count]) => (
                  <button
                    key={tag}
                    className="tag-filter__opt"
                    type="button"
                    aria-pressed={activeTag === tag}
                    onClick={() => setTagFilter(activeTag === tag ? null : tag)}
                  >
                    {tag} <span className="tag-filter__count">{count}</span>
                  </button>
                ))}
                {tagGroups.ungrouped > 0 && (
                  <button
                    className="tag-filter__opt"
                    type="button"
                    aria-pressed={activeTag === UNGROUPED}
                    onClick={() => setTagFilter(activeTag === UNGROUPED ? null : UNGROUPED)}
                  >
                    Ungrouped <span className="tag-filter__count">{tagGroups.ungrouped}</span>
                  </button>
                )}
              </div>
            )}
            {activeTag !== null && groupRows.length > 0 && (
              <div className="group-actions" role="group" aria-label="Actions for this group">
                <button
                  className="btn btn--sm btn--ghost"
                  type="button"
                  disabled={bulkBusy}
                  onClick={() => handleBulk("start", groupRows)}
                  title={`Start every tunnel in ${activeTag === UNGROUPED ? "Ungrouped" : activeTag}`}
                >
                  <Icon name="play" size={13} />
                  Start group
                </button>
                <button
                  className="btn btn--sm btn--ghost"
                  type="button"
                  disabled={bulkBusy}
                  onClick={() => handleBulk("stop", groupRows)}
                >
                  <Icon name="stop" size={13} />
                  Stop group
                </button>
                <button
                  className="btn btn--sm btn--danger-ghost"
                  type="button"
                  disabled={bulkBusy}
                  onClick={() => setDeleteTargets(groupRows)}
                >
                  <Icon name="trash" size={13} />
                  Delete group
                </button>
              </div>
            )}
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
              onStart={(id) => startOne(id, startTunnel)}
              onStop={(id) => runAction(id, stopTunnel)}
              onRestart={(id) => startOne(id, restartTunnel)}
              onRequestDelete={(row) => setDeleteTargets([row])}
              onEdit={(row) => {
                setDraft(null);
                setSaveError(null);
                setFormFocus("name");
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
            onStart={() => handleBulk("start", selectedRows)}
            onStop={() => handleBulk("stop", selectedRows)}
            onRestart={() => handleBulk("restart", selectedRows)}
            onDelete={() => setDeleteTargets(selectedRows)}
            onClear={() => setSelection(new Set())}
          />
        </main>
      </div>

      <PortConflictModal
        view={conflict}
        onCancel={() => setConflict(null)}
        onTakeOver={takeOver}
        onChangePort={changePortFor}
      />

      <CloseAppModal
        open={closeOpen}
        running={stats.running}
        onCancel={() => setCloseOpen(false)}
        onChoose={chooseClose}
      />

      <ImportJsonModal
        open={importOpen}
        onCancel={() => setImportOpen(false)}
        onImport={handleImportJson}
      />

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
            tagSuggestions={tagSuggestions}
            focusField={formFocus}
          />
        )}
      </Drawer>

      <Toasts items={toasts} />
    </>
  );
}
