import { useEffect, useRef, useState } from "react";
import type { PortConflict } from "../lib/api";
import { usePresence } from "../lib/presence";
import { Icon } from "./Icon";

export interface ConflictView {
  conflict: PortConflict;
  /** The tunnel the user tried to start. */
  tunnelName: string;
}

/**
 * Shown instead of a generic Failed when a start is refused because the
 * local port is taken. Names the holder; for a managed tunnel offers to
 * stop it and start this one, otherwise to change this tunnel's port.
 */
export function PortConflictModal({
  view,
  onCancel,
  onTakeOver,
  onChangePort,
}: {
  view: ConflictView | null;
  onCancel: () => void;
  /** Only offered when the holder is a managed tunnel. */
  onTakeOver: (holderId: string) => Promise<void>;
  onChangePort: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const last = useRef<ConflictView | null>(view);
  if (view) last.current = view;
  const { mounted, shown } = usePresence(view !== null, 140);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const dialog = ref.current;
    if (mounted && dialog && !dialog.open) dialog.showModal();
    if (!mounted) setBusy(false);
  }, [mounted]);

  const current = last.current;
  if (!mounted || !current) return null;

  const { conflict, tunnelName } = current;
  const { owner } = conflict;
  const endpoint = `${conflict.bind}:${conflict.port}`;

  let holder: string;
  if (owner.kind === "tunnel") {
    holder = `the tunnel “${owner.name}”${owner.pid != null ? ` (pid ${owner.pid})` : ""}`;
  } else if (owner.process && owner.pid != null) {
    holder = `${owner.process} (pid ${owner.pid}), a process outside tnls`;
  } else if (owner.pid != null) {
    holder = `process ${owner.pid}, outside tnls`;
  } else {
    holder = "an unknown process (the OS did not say which)";
  }

  return (
    <dialog
      ref={ref}
      className={`modal${shown ? " is-open" : ""}`}
      aria-labelledby="conflict-title"
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) onCancel();
      }}
    >
      <div className="modal__body">
        <div className="modal__head">
          <span className="modal__icon" aria-hidden="true">
            <Icon name="alert" size={17} />
          </span>
          <div>
            <h2 className="modal__title" id="conflict-title">
              Port {conflict.port} is already in use
            </h2>
            <p className="modal__text">
              “{tunnelName}” was not started: {endpoint} is held by {holder}. A taken port is
              a configuration problem, so tnls does not retry it.
            </p>
          </div>
        </div>
        <div className="modal__target">
          <span>
            {endpoint} ·{" "}
            {owner.kind === "tunnel"
              ? owner.name
              : owner.process ?? (owner.pid != null ? `pid ${owner.pid}` : "unknown process")}
          </span>
          {owner.kind === "tunnel" &&
            owner.tags.map((t) => (
              <span key={t} className="tag-chip">
                {t}
              </span>
            ))}
        </div>
      </div>
      <div className="modal__foot">
        <button className="btn btn--ghost" type="button" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
        <span className="modal__foot-spacer" />
        {owner.kind === "tunnel" ? (
          <>
            <button className="btn btn--ghost" type="button" disabled={busy} onClick={onChangePort}>
              Change port
            </button>
            <button
              className="btn btn--primary"
              type="button"
              autoFocus
              disabled={busy}
              title={`Stop “${owner.name}”, then start “${tunnelName}” on ${endpoint}`}
              onClick={async () => {
                setBusy(true);
                try {
                  await onTakeOver(owner.id);
                } finally {
                  setBusy(false);
                }
              }}
            >
              {busy ? "Switching…" : "Stop it and start this one"}
            </button>
          </>
        ) : (
          <button className="btn btn--primary" type="button" autoFocus onClick={onChangePort}>
            Change port
          </button>
        )}
      </div>
    </dialog>
  );
}
