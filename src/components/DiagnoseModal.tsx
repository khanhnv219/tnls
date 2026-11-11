import { useCallback, useEffect, useRef, useState } from "react";
import {
  diagnoseTunnel,
  onDiagnoseStage,
  type DiagOutcome,
  type DiagStage,
  type Diagnosis,
  type StageReport,
} from "../lib/api";
import { usePresence } from "../lib/presence";
import type { TunnelState } from "../lib/types";
import { Icon } from "./Icon";
import type { PushToast } from "./Toasts";

const STAGES: { id: DiagStage; label: string; what: string }[] = [
  { id: "sshAuth", label: "SSH auth / reachability", what: "connect and authenticate, no forwarding" },
  { id: "localForward", label: "Local forward", what: "can the local bind:port be opened" },
  { id: "destination", label: "Destination reachability", what: "can the server open remote host:port" },
];

/** Outcome shown with the status-badge palette (state reinforced by word + icon). */
const BADGE: Record<DiagOutcome | "running" | "pending", { state: TunnelState; label: string }> = {
  pass: { state: "running", label: "Pass" },
  fail: { state: "failed", label: "Fail" },
  skipped: { state: "stopped", label: "Skipped" },
  notRun: { state: "stopped", label: "Not run" },
  running: { state: "starting", label: "Checking" },
  pending: { state: "stopped", label: "Waiting" },
};

function OutcomeBadge({ kind }: { kind: keyof typeof BADGE }) {
  const { state, label } = BADGE[kind];
  const icon = kind === "pass" ? "check" : kind === "fail" ? "alert" : "circle";
  return (
    <span className="badge" data-state={state}>
      {kind === "running" ? (
        <Icon name="loader" size={13} className="spinner" />
      ) : (
        <Icon name={icon} size={13} className="badge__icon" />
      )}
      <span className="badge__text">{label}</span>
    </span>
  );
}

/**
 * Runs the three diagnostic stages for one tunnel and shows each as it
 * finishes. Read-only: the tunnel keeps whatever state it was in.
 */
export function DiagnoseModal({
  target,
  onClose,
  notify,
}: {
  target: { id: string; name: string } | null;
  onClose: () => void;
  notify: PushToast;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const last = useRef(target);
  if (target) last.current = target;
  const { mounted, shown } = usePresence(target !== null, 140);
  const [reports, setReports] = useState<Partial<Record<DiagStage, StageReport>>>({});
  const [result, setResult] = useState<Diagnosis | null>(null);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const runId = useRef(0);

  const run = useCallback(async (id: string) => {
    const mine = ++runId.current;
    setReports({});
    setResult(null);
    setError(null);
    setRunning(true);
    try {
      const d = await diagnoseTunnel(id);
      if (runId.current !== mine) return;
      setResult(d);
      setReports(Object.fromEntries(d.stages.map((s) => [s.stage, s])));
    } catch (e) {
      if (runId.current === mine) setError(String(e));
    } finally {
      if (runId.current === mine) setRunning(false);
    }
  }, []);

  useEffect(() => {
    const dialog = ref.current;
    if (mounted && dialog && !dialog.open) dialog.showModal();
  }, [mounted]);

  // Start a run whenever a new target opens the dialog.
  useEffect(() => {
    if (target) void run(target.id);
    else runId.current += 1; // drop late results after close
  }, [target, run]);

  // Fill stages live from backend events while the command is running.
  useEffect(() => {
    if (!target) return;
    let unlisten: (() => void) | undefined;
    onDiagnoseStage((tunnelId, report) => {
      if (tunnelId !== target.id) return;
      setReports((cur) => ({ ...cur, [report.stage]: report }));
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});
    return () => unlisten?.();
  }, [target]);

  const view = last.current;
  if (!mounted || !view) return null;

  const firstOpen = STAGES.findIndex((s) => !reports[s.id]);
  const failed = result?.stages.find((s) => s.outcome === "fail");

  return (
    <dialog
      ref={ref}
      className={`modal modal--wide${shown ? " is-open" : ""}`}
      aria-labelledby="diag-title"
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
    >
      <div className="modal__body">
        <div className="modal__head">
          <span className="modal__icon modal__icon--neutral" aria-hidden="true">
            <Icon name="layers" size={17} />
          </span>
          <div>
            <h2 className="modal__title" id="diag-title">
              Diagnose — {view.name}
            </h2>
            <p className="modal__text">
              Three checks in order, stopping at the first failure. Read-only: the tunnel is not
              started, stopped or restarted.
            </p>
          </div>
        </div>

        <ol className="diag-list">
          {STAGES.map((s, i) => {
            const r = reports[s.id];
            const kind = r ? r.outcome : running && i === firstOpen ? "running" : "pending";
            return (
              <li key={s.id} className="diag-stage" data-outcome={kind}>
                <span className="diag-stage__index">{i + 1}</span>
                <div className="diag-stage__body">
                  <div className="diag-stage__title">
                    <span>{s.label}</span>
                    {r && r.elapsedMs > 0 && (
                      <span className="diag-stage__time">{(r.elapsedMs / 1000).toFixed(1)}s</span>
                    )}
                  </div>
                  <p className="diag-stage__reason">{r ? r.reason : s.what}</p>
                  {r && r.outcome === "fail" && r.detail.length > 0 && (
                    <pre className="diag-stage__detail">{r.detail.slice(-6).join("\n")}</pre>
                  )}
                </div>
                <OutcomeBadge kind={kind} />
              </li>
            );
          })}
        </ol>

        {error && (
          <p className="form-error" role="alert">
            <Icon name="alert" size={14} />
            <span>{error}</span>
          </p>
        )}
        {result && !failed && !error && (
          <p className="diag-verdict">All checks passed for the stages that apply to this tunnel.</p>
        )}
      </div>
      <div className="modal__foot">
        <button
          className="btn btn--ghost"
          type="button"
          disabled={!result}
          onClick={async () => {
            if (!result) return;
            try {
              await navigator.clipboard.writeText(result.summary);
              notify("Diagnostics summary copied", "success");
            } catch {
              notify("Clipboard unavailable", "warn");
            }
          }}
        >
          <Icon name="copy" size={14} />
          Copy summary
        </button>
        <span className="modal__foot-spacer" />
        <button
          className="btn btn--ghost"
          type="button"
          disabled={running}
          onClick={() => void run(view.id)}
        >
          <Icon name="refresh" size={14} />
          Run again
        </button>
        <button className="btn btn--primary" type="button" autoFocus onClick={onClose}>
          Close
        </button>
      </div>
    </dialog>
  );
}
