import { useRef } from "react";
import { usePresence } from "../lib/presence";
import { Icon } from "./Icon";

/** Slides up over the bottom of the table while rows are selected. */
export function BulkBar({
  count,
  busy,
  onStart,
  onStop,
  onRestart,
  onDelete,
  onClear,
}: {
  count: number;
  busy: boolean;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onDelete: () => void;
  onClear: () => void;
}) {
  const { mounted, shown } = usePresence(count > 0, 180);
  // Keep the last non-zero count on screen while the bar slides away.
  const lastCount = useRef(count);
  if (count > 0) lastCount.current = count;
  if (!mounted) return null;

  return (
    <div className={`bulkbar${shown ? " is-open" : ""}`} role="region" aria-label="Bulk actions">
      <span className="bulkbar__count">
        <strong>{lastCount.current}</strong> selected
      </span>
      <button className="btn btn--sm btn--ghost" type="button" disabled={busy} onClick={onStart}>
        <Icon name="play" size={13} />
        Start
      </button>
      <button className="btn btn--sm btn--ghost" type="button" disabled={busy} onClick={onStop}>
        <Icon name="stop" size={13} />
        Stop
      </button>
      <button className="btn btn--sm btn--ghost" type="button" disabled={busy} onClick={onRestart}>
        <Icon name="restart" size={13} />
        Restart
      </button>
      <button className="btn btn--sm btn--danger-ghost" type="button" disabled={busy} onClick={onDelete}>
        <Icon name="trash" size={13} />
        Delete
      </button>
      <div className="bulkbar__spacer" />
      <button className="btn btn--sm btn--ghost" type="button" onClick={onClear}>
        Clear selection
      </button>
    </div>
  );
}
