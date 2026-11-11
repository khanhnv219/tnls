import { useEffect, useRef } from "react";
import { usePresence } from "../lib/presence";
import { Icon } from "./Icon";

export interface ConfirmContent {
  title: string;
  text: string;
  /** Mono line naming exactly what will be affected. */
  target: string;
  confirmLabel: string;
}

/**
 * Destructive confirmation on a native <dialog>. Pass `content = null` to
 * close; the last content stays rendered while the exit transition runs.
 */
export function ConfirmModal({
  content,
  onCancel,
  onConfirm,
}: {
  content: ConfirmContent | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const last = useRef<ConfirmContent | null>(content);
  if (content) last.current = content;
  const { mounted, shown } = usePresence(content !== null, 140);

  useEffect(() => {
    const dialog = ref.current;
    if (mounted && dialog && !dialog.open) dialog.showModal();
  }, [mounted]);

  const view = last.current;
  if (!mounted || !view) return null;

  return (
    <dialog
      ref={ref}
      className={`modal${shown ? " is-open" : ""}`}
      aria-labelledby="confirm-title"
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
    >
      <div className="modal__body">
        <div className="modal__head">
          <span className="modal__icon" aria-hidden="true">
            <Icon name="trash" size={17} />
          </span>
          <div>
            <h2 className="modal__title" id="confirm-title">
              {view.title}
            </h2>
            <p className="modal__text">{view.text}</p>
          </div>
        </div>
        <div className="modal__target">
          <span>{view.target}</span>
        </div>
      </div>
      <div className="modal__foot">
        <span className="modal__foot-spacer" />
        <button className="btn btn--ghost" type="button" autoFocus onClick={onCancel}>
          Cancel
        </button>
        <button className="btn btn--danger" type="button" onClick={onConfirm}>
          {view.confirmLabel}
        </button>
      </div>
    </dialog>
  );
}
