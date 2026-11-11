import { useEffect, useRef, useState } from "react";
import { usePresence } from "../lib/presence";
import { Icon } from "./Icon";

/** Paste a JSON array of tunnel profiles (the format "Copy all as JSON" produces). */
export function ImportJsonModal({
  open,
  onCancel,
  onImport,
}: {
  open: boolean;
  onCancel: () => void;
  /** Resolves on success; rejects with the backend's message on failure. */
  onImport: (json: string) => Promise<void>;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const { mounted, shown } = usePresence(open, 140);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (mounted && dialog && !dialog.open) dialog.showModal();
    if (!mounted) {
      setText("");
      setError(null);
    }
  }, [mounted]);

  if (!mounted) return null;

  const submit = async () => {
    if (text.trim() === "") {
      setError("Paste the JSON first.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onImport(text);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <dialog
      ref={ref}
      className={`modal${shown ? " is-open" : ""}`}
      aria-labelledby="import-title"
      onCancel={(e) => {
        e.preventDefault();
        if (!busy) onCancel();
      }}
    >
      <div className="modal__body">
        <div className="modal__head">
          <span className="modal__icon modal__icon--neutral" aria-hidden="true">
            <Icon name="upload" size={17} />
          </span>
          <div>
            <h2 className="modal__title" id="import-title">
              Import tunnels from JSON
            </h2>
            <p className="modal__text">
              Paste an array of tunnel profiles, for example the output of “Copy all as
              JSON”. Every entry becomes a new profile; nothing is overwritten.
            </p>
          </div>
        </div>
        <textarea
          className={`input input--mono${error ? " is-invalid" : ""}`}
          aria-label="Tunnels JSON"
          aria-invalid={error ? true : undefined}
          spellCheck={false}
          autoFocus
          placeholder='[{ "name": "Dev PostgreSQL", "mode": "local", "sshTarget": "dev-bastion", "localPort": 5433, "remoteHost": "postgres.internal", "remotePort": 5432 }]'
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
        />
        {error && (
          <p className="field__error" role="alert">
            <Icon name="alert" size={13} />
            <span>{error}</span>
          </p>
        )}
      </div>
      <div className="modal__foot">
        <span className="modal__foot-spacer" />
        <button className="btn btn--ghost" type="button" disabled={busy} onClick={onCancel}>
          Cancel
        </button>
        <button className="btn btn--primary" type="button" disabled={busy} onClick={submit}>
          {busy ? "Importing…" : "Import"}
        </button>
      </div>
    </dialog>
  );
}
