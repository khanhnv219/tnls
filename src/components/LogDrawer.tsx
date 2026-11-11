import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { getTunnelLogs } from "../lib/api";
import { clockTime, type LogLevel, type LogLine } from "../lib/logs";
import { Icon } from "./Icon";
import type { PushToast } from "./Toasts";

const LEVELS: { value: LogLevel | "ALL"; label: string }[] = [
  { value: "ALL", label: "ALL" },
  { value: "ERROR", label: "ERR" },
  { value: "WARN", label: "WARN" },
  { value: "INFO", label: "INFO" },
  { value: "DEBUG", label: "DBG" },
];

const POLL_MS = 2000;
const FETCH_LIMIT = 400;

function highlight(text: string, query: string) {
  if (query === "") return text;
  const lower = text.toLowerCase();
  const q = query.toLowerCase();
  const parts: ReactNode[] = [];
  let from = 0;
  let at = lower.indexOf(q);
  while (at !== -1) {
    if (at > from) parts.push(text.slice(from, at));
    parts.push(
      <span key={at} className="logline__mark">
        {text.slice(at, at + q.length)}
      </span>,
    );
    from = at + q.length;
    at = lower.indexOf(q, from);
  }
  if (from < text.length) parts.push(text.slice(from));
  return parts.map((p, i) => <Fragment key={i}>{p}</Fragment>);
}

/**
 * Head and body rows for the log drawer: a console of the tunnel's ssh
 * stderr (time and level come from the backend), polled while open, with
 * level filter, text filter and follow mode.
 */
export function LogDrawer({
  tunnelId,
  tunnelName,
  onClose,
  notify,
}: {
  tunnelId: string;
  tunnelName: string;
  onClose: () => void;
  notify: PushToast;
}) {
  const [lines, setLines] = useState<LogLine[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [level, setLevel] = useState<LogLevel | "ALL">("ALL");
  const [filter, setFilter] = useState("");
  const [follow, setFollow] = useState(true);
  const [awayFromBottom, setAwayFromBottom] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  const fetchLogs = useRef(async () => {});
  fetchLogs.current = async () => {
    try {
      setLines(await getTunnelLogs(tunnelId, FETCH_LIMIT));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  useEffect(() => {
    void fetchLogs.current();
    const id = setInterval(() => void fetchLogs.current(), POLL_MS);
    return () => clearInterval(id);
  }, [tunnelId]);

  useEffect(() => {
    const t = setTimeout(() => filterRef.current?.focus(), 180);
    return () => clearTimeout(t);
  }, []);

  const parsed = useMemo(
    () =>
      (lines ?? []).map((l, i) => ({
        n: i,
        time: clockTime(l.at),
        at: l.at,
        msg: l.text,
        level: l.level,
      })),
    [lines],
  );

  const q = filter.trim();
  const visible = useMemo(() => {
    const ql = q.toLowerCase();
    return parsed.filter(
      (l) =>
        (level === "ALL" || l.level === level) &&
        (ql === "" || l.msg.toLowerCase().includes(ql)),
    );
  }, [parsed, level, q]);

  const counts = useMemo(() => {
    const c: Record<LogLevel, number> = { ERROR: 0, WARN: 0, INFO: 0, DEBUG: 0 };
    parsed.forEach((l) => (c[l.level] += 1));
    return c;
  }, [parsed]);

  useEffect(() => {
    if (!follow || !scrollRef.current) return;
    scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  }, [visible, follow]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAwayFromBottom(!nearBottom);
    // Scrolling up to read detaches follow; "Jump to latest" re-attaches it.
    if (!nearBottom && follow) setFollow(false);
  };

  const jumpToLatest = () => {
    setFollow(true);
    setAwayFromBottom(false);
    if (scrollRef.current) scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
  };

  const copyVisible = async () => {
    try {
      await navigator.clipboard.writeText(
        visible.map((l) => `${l.time} ${l.level} ${l.msg}`).join("\n"),
      );
      notify(`${visible.length} log ${visible.length === 1 ? "line" : "lines"} copied`, "success");
    } catch {
      notify("Clipboard unavailable", "warn");
    }
  };

  const emptyText =
    lines === null
      ? "Loading…"
      : parsed.length === 0
        ? "No log lines yet. Start the tunnel to see output."
        : "No lines match this filter.";

  return (
    <>
      <header className="drawer__head">
        <div className="drawer__title-stack">
          <span className="drawer__eyebrow">Tunnel logs</span>
          <h2 className="drawer__title" id="log-title">
            {tunnelName}
          </h2>
        </div>
        <button
          className="btn btn--icon"
          type="button"
          aria-label="Copy visible lines"
          title="Copy visible lines"
          disabled={visible.length === 0}
          onClick={copyVisible}
        >
          <Icon name="download" size={16} />
        </button>
        <button className="btn btn--icon" type="button" aria-label="Close logs" onClick={onClose}>
          <Icon name="x" size={16} />
        </button>
      </header>

      <div className="logview">
        <div className="logbar">
          <div className="logbar__search">
            <Icon name="search" size={13} className="search__icon" />
            <input
              ref={filterRef}
              className="input"
              type="text"
              placeholder="Filter lines"
              aria-label="Filter log lines"
              autoComplete="off"
              spellCheck={false}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>

          <div className="loglevels" role="group" aria-label="Filter by log level">
            {LEVELS.map((l) => (
              <button
                key={l.value}
                className="loglevels__opt"
                type="button"
                data-level={l.value}
                aria-pressed={level === l.value}
                onClick={() => setLevel(l.value)}
              >
                {l.label}
              </button>
            ))}
          </div>

          <button
            className="log-follow"
            type="button"
            aria-pressed={follow}
            title="Follow new lines"
            onClick={() => (follow ? setFollow(false) : jumpToLatest())}
          >
            <span className="log-follow__dot" aria-hidden="true" />
            Live
          </button>
          <button
            className="btn btn--icon"
            type="button"
            aria-label="Refresh logs"
            title="Refresh"
            onClick={() => void fetchLogs.current()}
          >
            <Icon name="refresh" size={15} />
          </button>
        </div>

        {error && (
          <p className="form-error" role="alert" style={{ margin: "var(--s-3) var(--s-4)" }}>
            <Icon name="alert" size={14} />
            <span>{error}</span>
          </p>
        )}

        {visible.length > 0 ? (
          <div
            ref={scrollRef}
            className="loglines"
            tabIndex={0}
            role="log"
            aria-live="polite"
            aria-label="Log output"
            onScroll={onScroll}
          >
            {visible.map((l) => (
              <div key={l.n} className="logline" data-level={l.level}>
                <span className="logline__time" title={new Date(l.at).toLocaleString()}>
                  {l.time}
                </span>
                <span className="logline__level">{l.level}</span>
                <span className="logline__msg">{highlight(l.msg, q)}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="log-empty">
            <Icon name="terminal" size={24} />
            <span>{emptyText}</span>
          </div>
        )}

        <button
          className={`jump-bottom${awayFromBottom && visible.length > 0 ? " is-shown" : ""}`}
          type="button"
          onClick={jumpToLatest}
        >
          <Icon name="arrow-down" size={13} />
          Jump to latest
        </button>

        <div className="log-stat">
          {visible.length}/{parsed.length} lines · {counts.ERROR} err · {counts.WARN} warn ·{" "}
          {counts.INFO} info · {counts.DEBUG} debug
        </div>
      </div>
    </>
  );
}
