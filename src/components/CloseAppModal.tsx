import { useEffect, useRef, useState } from "react";
import type { CloseAction } from "../lib/closeAction";
import { usePresence } from "../lib/presence";
import { Icon } from "./Icon";

/**
 * Asked when the window's close button is pressed: keep tunnels running in
 * the tray, or quit and stop them. "Remember" stores the answer.
 */
export function CloseAppModal({
  open,
  running,
  onCancel,
  onChoose,
}: {
  open: boolean;
  running: number;
  onCancel: () => void;
  onChoose: (action: Exclude<CloseAction, "ask">, remember: boolean) => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const { mounted, shown } = usePresence(open, 140);
  const [remember, setRemember] = useState(false);

  useEffect(() => {
    const dialog = ref.current;
    if (mounted && dialog && !dialog.open) dialog.showModal();
    if (!mounted) setRemember(false);
  }, [mounted]);

  if (!mounted) return null;

  const text =
    running > 0
      ? `${running} ${running === 1 ? "tunnel is" : "tunnels are"} running. Minimize keeps ${running === 1 ? "it" : "them"} up in the background; reopen tnls from the tray icon. Quit stops every tunnel.`
      : "Minimize keeps tnls in the tray so auto-start and tray actions stay available. Quit closes it completely.";

  return (
    <dialog
      ref={ref}
      className={`modal${shown ? " is-open" : ""}`}
      aria-labelledby="close-title"
      onCancel={(e) => {
        e.preventDefault();
        onCancel();
      }}
    >
      <div className="modal__body">
        <div className="modal__head">
          <span className="modal__icon modal__icon--neutral" aria-hidden="true">
            <Icon name="layers" size={17} />
          </span>
          <div>
            <h2 className="modal__title" id="close-title">
              Close tnls?
            </h2>
            <p className="modal__text">{text}</p>
          </div>
        </div>
        <label className="modal__remember">
          <input
            type="checkbox"
            className="check"
            checked={remember}
            onChange={(e) => setRemember(e.target.checked)}
          />
          Remember my choice
          <span className="field__optional">change it later in the ⋯ menu</span>
        </label>
      </div>
      <div className="modal__foot">
        <button className="btn btn--ghost" type="button" onClick={onCancel}>
          Cancel
        </button>
        <span className="modal__foot-spacer" />
        <button
          className="btn btn--danger-ghost"
          type="button"
          onClick={() => onChoose("quit", remember)}
        >
          <Icon name="stop" size={13} />
          Quit tnls
        </button>
        <button
          className="btn btn--primary"
          type="button"
          autoFocus
          onClick={() => onChoose("tray", remember)}
        >
          Minimize to tray
        </button>
      </div>
    </dialog>
  );
}
