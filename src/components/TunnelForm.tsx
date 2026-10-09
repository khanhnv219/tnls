import { useState } from "react";
import type { TunnelConfig, TunnelInput, TunnelMode } from "../lib/types";

const MODES: { value: TunnelMode; label: string }[] = [
  { value: "local", label: "Local (-L)" },
  { value: "remote", label: "Remote (-R)" },
  { value: "dynamic", label: "Dynamic SOCKS (-D)" },
];

function parsePort(raw: string): number | null {
  if (!/^\d{1,5}$/.test(raw.trim())) return null;
  const n = Number(raw.trim());
  return n >= 1 && n <= 65535 ? n : null;
}

export function TunnelForm({
  initial,
  saving,
  saveError,
  onSave,
  onClose,
}: {
  initial: TunnelConfig | null;
  saving: boolean;
  saveError: string | null;
  onSave: (input: TunnelInput) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(initial?.name ?? "");
  const [mode, setMode] = useState<TunnelMode>(initial?.mode ?? "local");
  const [sshTarget, setSshTarget] = useState(initial?.sshTarget ?? "");
  const [bindAddress, setBindAddress] = useState(
    initial?.bindAddress ?? "127.0.0.1",
  );
  const [localPort, setLocalPort] = useState(
    initial ? String(initial.localPort) : "",
  );
  const [remoteHost, setRemoteHost] = useState(initial?.remoteHost ?? "");
  const [remotePort, setRemotePort] = useState(
    initial?.remotePort != null ? String(initial.remotePort) : "",
  );
  const [identityFile, setIdentityFile] = useState(
    initial?.identityFile ?? "",
  );
  const [autoStart, setAutoStart] = useState(initial?.autoStart ?? false);
  const [keepAlive, setKeepAlive] = useState(
    initial ? String(initial.keepAliveInterval) : "30",
  );
  const [errors, setErrors] = useState<Record<string, string>>({});

  const needsRemote = mode === "local" || mode === "remote";

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const next: Record<string, string> = {};
    if (name.trim() === "") next.name = "Name is required.";
    if (sshTarget.trim() === "")
      next.sshTarget = "SSH target is required (user@host or ~/.ssh/config alias).";
    if (bindAddress.trim() === "") next.bindAddress = "Bind address is required.";
    const local = parsePort(localPort);
    if (local === null) next.localPort = "Local port must be 1–65535.";
    if (needsRemote) {
      if (remoteHost.trim() === "")
        next.remoteHost = "Remote host is required for local/remote forwarding.";
      if (parsePort(remotePort) === null)
        next.remotePort = "Remote port must be 1–65535.";
    }
    const keep = parsePort(keepAlive);
    if (keep === null) next.keepAlive = "Keep-alive must be 1–65535 seconds.";
    setErrors(next);
    if (Object.keys(next).length > 0) return;

    onSave({
      name: name.trim(),
      mode,
      sshTarget: sshTarget.trim(),
      bindAddress: bindAddress.trim() === "" ? null : bindAddress.trim(),
      localPort: local as number,
      remoteHost: needsRemote ? remoteHost.trim() : null,
      remotePort: needsRemote ? (parsePort(remotePort) as number) : null,
      identityFile: identityFile.trim() === "" ? null : identityFile.trim(),
      autoStart,
      keepAliveInterval: keep as number,
    });
  }

  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside
        className="drawer form-drawer"
        onClick={(e) => e.stopPropagation()}
        aria-label={initial ? `Edit ${initial.name}` : "New tunnel"}
      >
        <header className="drawer-head">
          <h2>{initial ? `Edit — ${initial.name}` : "New tunnel"}</h2>
          <button onClick={onClose} aria-label="Close form">
            ✕
          </button>
        </header>
        <form className="tunnel-form" onSubmit={submit}>
          <label>
            Name
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Dev PostgreSQL"
              autoFocus
            />
            {errors.name && <span className="field-error">{errors.name}</span>}
          </label>

          <label>
            Mode
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value as TunnelMode)}
            >
              {MODES.map((m) => (
                <option key={m.value} value={m.value}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>

          <label>
            SSH target
            <input
              value={sshTarget}
              onChange={(e) => setSshTarget(e.target.value)}
              placeholder="dev-bastion or deploy@bastion.example.com"
            />
            {errors.sshTarget && (
              <span className="field-error">{errors.sshTarget}</span>
            )}
          </label>

          <div className="form-row">
            <label>
              Bind address
              <input
                value={bindAddress}
                onChange={(e) => setBindAddress(e.target.value)}
                placeholder="127.0.0.1"
              />
              {errors.bindAddress && (
                <span className="field-error">{errors.bindAddress}</span>
              )}
            </label>
            <label>
              Local port
              <input
                value={localPort}
                onChange={(e) => setLocalPort(e.target.value)}
                placeholder="5433"
                inputMode="numeric"
              />
              {errors.localPort && (
                <span className="field-error">{errors.localPort}</span>
              )}
            </label>
          </div>

          {needsRemote && (
            <div className="form-row">
              <label>
                Remote host
                <input
                  value={remoteHost}
                  onChange={(e) => setRemoteHost(e.target.value)}
                  placeholder="postgres.internal"
                />
                {errors.remoteHost && (
                  <span className="field-error">{errors.remoteHost}</span>
                )}
              </label>
              <label>
                Remote port
                <input
                  value={remotePort}
                  onChange={(e) => setRemotePort(e.target.value)}
                  placeholder="5432"
                  inputMode="numeric"
                />
                {errors.remotePort && (
                  <span className="field-error">{errors.remotePort}</span>
                )}
              </label>
            </div>
          )}

          <label>
            Identity file (optional)
            <input
              value={identityFile}
              onChange={(e) => setIdentityFile(e.target.value)}
              placeholder="~/.ssh/id_ed25519"
            />
          </label>

          <div className="form-row">
            <label>
              Keep-alive (s)
              <input
                value={keepAlive}
                onChange={(e) => setKeepAlive(e.target.value)}
                placeholder="30"
                inputMode="numeric"
              />
              {errors.keepAlive && (
                <span className="field-error">{errors.keepAlive}</span>
              )}
            </label>
            <label className="check">
              <input
                type="checkbox"
                checked={autoStart}
                onChange={(e) => setAutoStart(e.target.checked)}
              />
              Auto-start with app
            </label>
          </div>

          {saveError && (
            <p className="error" role="alert">
              {saveError}
            </p>
          )}

          <footer className="form-foot">
            <button type="button" className="ghost" onClick={onClose}>
              Cancel
            </button>
            <button type="submit" disabled={saving}>
              {saving ? "Saving…" : initial ? "Save changes" : "Create tunnel"}
            </button>
          </footer>
        </form>
      </aside>
    </div>
  );
}
