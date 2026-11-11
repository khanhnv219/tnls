import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { deletePassphrase, getPassphraseState } from "../lib/api";
import type { TunnelConfig, TunnelInput, TunnelMode } from "../lib/types";
import { CommandPreview } from "./CommandPreview";
import { TagInput } from "./TagInput";
import { Icon } from "./Icon";
import type { PushToast } from "./Toasts";

const MODES: { value: TunnelMode; label: string; flag: string }[] = [
  { value: "local", label: "Local", flag: "-L" },
  { value: "remote", label: "Remote", flag: "-R" },
  { value: "dynamic", label: "Dynamic", flag: "-D SOCKS" },
];

const MODE_HINT: Record<TunnelMode, string> = {
  local: "Local forward: expose a remote service on your machine.",
  remote: "Remote forward: expose your local service on the server.",
  dynamic: "Dynamic SOCKS: use the SSH host as a proxy for any destination.",
};

type FieldKey =
  | "name"
  | "sshTarget"
  | "bindAddress"
  | "localPort"
  | "remoteHost"
  | "remotePort"
  | "keepAlive"
  | "maxRetries"
  | "retryDelay";

const FIELD_ID: Record<FieldKey, string> = {
  name: "f-name",
  sshTarget: "f-target",
  bindAddress: "f-bind",
  localPort: "f-lport",
  remoteHost: "f-rhost",
  remotePort: "f-rport",
  keepAlive: "f-keepalive",
  maxRetries: "f-retries",
  retryDelay: "f-delay",
};

const ADVANCED_FIELDS: ReadonlySet<FieldKey> = new Set(["keepAlive", "maxRetries", "retryDelay"]);

const isPort = (raw: string) => {
  if (!/^\d{1,5}$/.test(raw.trim())) return false;
  const n = Number(raw.trim());
  return n >= 1 && n <= 65535;
};

const isUint = (raw: string, max: number) =>
  /^\d+$/.test(raw.trim()) && Number(raw.trim()) <= max;

interface Values {
  name: string;
  mode: TunnelMode;
  sshTarget: string;
  bindAddress: string;
  localPort: string;
  remoteHost: string;
  remotePort: string;
  keepAlive: string;
  maxRetries: string;
  retryDelay: string;
}

/** One field's error message, or null when the value is acceptable. */
function check(key: FieldKey, v: Values): string | null {
  const needsRemote = v.mode !== "dynamic";
  switch (key) {
    case "name":
      return v.name.trim() === ""
        ? "Give the tunnel a name — it is what you will search for later."
        : null;
    case "sshTarget":
      return v.sshTarget.trim() === ""
        ? "Enter user@host or an alias from ~/.ssh/config."
        : null;
    case "bindAddress":
      return v.bindAddress.trim() === "" ? "Enter a bind address, usually 127.0.0.1." : null;
    case "localPort":
      return isPort(v.localPort) ? null : "Local port must be a number from 1 to 65535.";
    case "remoteHost":
      return needsRemote && v.remoteHost.trim() === ""
        ? "Remote host is required for local and remote forwards."
        : null;
    case "remotePort":
      return needsRemote && !isPort(v.remotePort)
        ? "Remote port must be a number from 1 to 65535."
        : null;
    case "keepAlive":
      return isPort(v.keepAlive) ? null : "Keep-alive must be 1–65535 seconds.";
    case "maxRetries":
      return isUint(v.maxRetries, 20) ? null : "Max retries must be 0–20.";
    case "retryDelay":
      return isUint(v.retryDelay, 30) && Number(v.retryDelay) >= 1
        ? null
        : "Retry delay must be 1–30 seconds.";
  }
}

const ALL_FIELDS = Object.keys(FIELD_ID) as FieldKey[];

function FieldError({ id, message }: { id: string; message?: string }) {
  if (!message) return null;
  return (
    <p className="field__error" id={id}>
      <Icon name="alert" size={13} />
      <span>{message}</span>
    </p>
  );
}

/**
 * Head, body and foot rows for the form drawer. Create, edit and duplicate
 * share it: `initial` is the profile being edited, `draft` prefills a new one.
 */
export function TunnelForm({
  initial,
  draft,
  saving,
  saveError,
  onSave,
  onClose,
  notify,
  tagSuggestions,
  focusField,
}: {
  initial: TunnelConfig | null;
  draft?: TunnelInput | null;
  saving: boolean;
  saveError: string | null;
  onSave: (input: TunnelInput, passphrase: string) => Promise<void>;
  onClose: () => void;
  notify: PushToast;
  /** Tags already used by other tunnels. */
  tagSuggestions: string[];
  /** Field to focus on open; "localPort" when sent here by a port conflict. */
  focusField?: "name" | "localPort";
}) {
  const seed: Partial<TunnelInput> | null = draft ?? initial;
  const [values, setValues] = useState<Values>(() => ({
    name: seed?.name ?? "",
    mode: seed?.mode ?? "local",
    sshTarget: seed?.sshTarget ?? "",
    bindAddress: seed?.bindAddress ?? "127.0.0.1",
    localPort: seed?.localPort != null ? String(seed.localPort) : "",
    remoteHost: seed?.remoteHost ?? "",
    remotePort: seed?.remotePort != null ? String(seed.remotePort) : "",
    keepAlive: seed?.keepAliveInterval != null ? String(seed.keepAliveInterval) : "30",
    maxRetries: seed?.maxRetries != null ? String(seed.maxRetries) : "5",
    retryDelay: seed?.retryDelaySecs != null ? String(seed.retryDelaySecs) : "1",
  }));
  const [identityFile, setIdentityFile] = useState(seed?.identityFile ?? "");
  const [proxyJump, setProxyJump] = useState(seed?.proxyJump ?? "");
  const [autoStart, setAutoStart] = useState(seed?.autoStart ?? false);
  const [tags, setTags] = useState<string[]>(seed?.tags ?? []);
  const [passphrase, setPassphrase] = useState("");
  const [hasStored, setHasStored] = useState(false);
  const [errors, setErrors] = useState<Partial<Record<FieldKey, string>>>({});
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const nameRef = useRef<HTMLInputElement>(null);
  const modeRefs = useRef<Record<TunnelMode, HTMLButtonElement | null>>({
    local: null,
    remote: null,
    dynamic: null,
  });

  const needsRemote = values.mode !== "dynamic";

  // Best-effort input for the command preview: invalid numbers become
  // "missing" so the backend renders a placeholder instead of rejecting.
  const previewInput: TunnelInput = {
    name: values.name,
    mode: values.mode,
    sshTarget: values.sshTarget,
    bindAddress: values.bindAddress,
    localPort: isPort(values.localPort) ? Number(values.localPort.trim()) : 0,
    remoteHost: needsRemote ? values.remoteHost : null,
    remotePort: needsRemote && isPort(values.remotePort) ? Number(values.remotePort.trim()) : null,
    identityFile: identityFile.trim() === "" ? null : identityFile,
    proxyJump: proxyJump.trim() === "" ? null : proxyJump,
    keepAliveInterval: isPort(values.keepAlive) ? Number(values.keepAlive.trim()) : null,
  };

  useEffect(() => {
    // Wait for the drawer to slide in before moving focus into it.
    const t = setTimeout(() => {
      if (focusField === "localPort") {
        const el = document.getElementById(FIELD_ID.localPort) as HTMLInputElement | null;
        el?.focus();
        el?.select();
      } else {
        nameRef.current?.focus();
      }
    }, 180);
    return () => clearTimeout(t);
    // Runs once on open; focusField is fixed for the lifetime of the form.
  }, []);

  useEffect(() => {
    if (!initial?.id) return;
    getPassphraseState(initial.id)
      .then(setHasStored)
      .catch(() => {});
  }, [initial?.id]);

  const set = (key: keyof Values, value: string) => {
    setValues((v) => ({ ...v, [key]: value }));
    // Clear a field's error as soon as the user edits it.
    if (key in FIELD_ID) {
      setErrors((e) => (e[key as FieldKey] ? { ...e, [key]: undefined } : e));
    }
  };

  // Validate on blur, never per keystroke. Empty optional-on-blur fields stay quiet
  // until submit so tabbing through a fresh form does not light up in red.
  const blur = (key: FieldKey) => {
    const raw = values[key as keyof Values];
    if ((key === "localPort" || key === "remotePort") && raw === "") return;
    setErrors((e) => ({ ...e, [key]: check(key, values) ?? undefined }));
  };

  const setMode = (mode: TunnelMode) => {
    setValues((v) => ({ ...v, mode }));
    setErrors((e) => ({ ...e, remoteHost: undefined, remotePort: undefined }));
  };

  const onModeKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const idx = MODES.findIndex((m) => m.value === values.mode);
    const step = e.key === "ArrowRight" ? 1 : -1;
    const next = MODES[(idx + step + MODES.length) % MODES.length].value;
    setMode(next);
    modeRefs.current[next]?.focus();
  };

  async function submit(e: FormEvent) {
    e.preventDefault();
    const next: Partial<Record<FieldKey, string>> = {};
    for (const key of ALL_FIELDS) {
      const msg = check(key, values);
      if (msg) next[key] = msg;
    }
    setErrors(next);
    const invalid = ALL_FIELDS.filter((k) => next[k]);
    if (invalid.length > 0) {
      // Land on the field that is actually wrong, opening Advanced if needed.
      if (invalid.some((k) => ADVANCED_FIELDS.has(k))) setAdvancedOpen(true);
      const first = invalid[0];
      setTimeout(() => document.getElementById(FIELD_ID[first])?.focus(), 0);
      return;
    }

    try {
      await onSave(
        {
          name: values.name.trim(),
          mode: values.mode,
          sshTarget: values.sshTarget.trim(),
          bindAddress: values.bindAddress.trim(),
          localPort: Number(values.localPort.trim()),
          remoteHost: needsRemote ? values.remoteHost.trim() : null,
          remotePort: needsRemote ? Number(values.remotePort.trim()) : null,
          identityFile: identityFile.trim() === "" ? null : identityFile.trim(),
          proxyJump: proxyJump.trim() === "" ? null : proxyJump.trim(),
          autoStart,
          keepAliveInterval: Number(values.keepAlive.trim()),
          maxRetries: Number(values.maxRetries.trim()),
          retryDelaySecs: Number(values.retryDelay.trim()),
          tags,
        },
        passphrase,
      );
    } catch {
      // saveError is rendered from the parent.
    }
  }

  const input = (key: FieldKey, props: { placeholder?: string; mono?: boolean; numeric?: boolean }) => (
    <input
      className={`input${props.mono ? " input--mono" : ""}${errors[key] ? " is-invalid" : ""}`}
      id={FIELD_ID[key]}
      ref={key === "name" ? nameRef : undefined}
      type="text"
      autoComplete="off"
      spellCheck={false}
      inputMode={props.numeric ? "numeric" : undefined}
      placeholder={props.placeholder}
      value={values[key as keyof Values]}
      onChange={(e) => set(key as keyof Values, e.target.value)}
      onBlur={() => blur(key)}
      aria-invalid={errors[key] ? true : undefined}
      aria-describedby={errors[key] ? `${FIELD_ID[key]}-error` : undefined}
    />
  );

  const req = (
    <span className="field__req" aria-hidden="true">
      *
    </span>
  );

  return (
    <>
      <header className="drawer__head">
        <div className="drawer__title-stack">
          <span className="drawer__eyebrow">
            {initial ? "Edit profile" : draft ? "Duplicate profile" : "New profile"}
          </span>
          <h2 className="drawer__title" id="form-title">
            {initial ? initial.name : "New tunnel"}
          </h2>
        </div>
        <button className="btn btn--icon" type="button" aria-label="Close" onClick={onClose}>
          <Icon name="x" size={16} />
        </button>
      </header>

      <div className="drawer__body">
        <form className="form" id="tunnel-form" noValidate onSubmit={submit}>
          <section className="section">
            <div className="section__head">
              <span className="section__index">1</span>
              <h3 className="section__title">Basics</h3>
            </div>

            <div className="field">
              <label className="field__label" htmlFor="f-name">
                Name {req}
              </label>
              {input("name", { placeholder: "Dev PostgreSQL" })}
              <FieldError id="f-name-error" message={errors.name} />
            </div>

            <div className="field">
              <label className="field__label" htmlFor="f-tags">
                Tags <span className="field__optional">optional, Enter to add</span>
              </label>
              <TagInput id="f-tags" tags={tags} suggestions={tagSuggestions} onChange={setTags} />
            </div>

            <div className="field">
              <span className="field__label" id="mode-label">
                Mode
              </span>
              <div
                className="segmented"
                role="radiogroup"
                aria-labelledby="mode-label"
                onKeyDown={onModeKey}
              >
                {MODES.map((m) => {
                  const checked = values.mode === m.value;
                  return (
                    <button
                      key={m.value}
                      ref={(el) => {
                        modeRefs.current[m.value] = el;
                      }}
                      className="segmented__opt"
                      type="button"
                      role="radio"
                      aria-checked={checked}
                      tabIndex={checked ? 0 : -1}
                      onClick={() => setMode(m.value)}
                    >
                      <span className="segmented__label">{m.label}</span>
                      <span className="segmented__flag">{m.flag}</span>
                    </button>
                  );
                })}
              </div>
              <p className="field__hint">{MODE_HINT[values.mode]}</p>
            </div>
          </section>

          <section className="section">
            <div className="section__head">
              <span className="section__index">2</span>
              <h3 className="section__title">Connection</h3>
              <span className="section__note">Addresses and ports</span>
            </div>

            <div className="field">
              <label className="field__label" htmlFor="f-target">
                SSH target {req}
              </label>
              {input("sshTarget", {
                placeholder: "deploy@bastion.example.com  or  dev-bastion",
                mono: true,
              })}
              <p className="field__hint">
                Use a <code>~/.ssh/config</code> alias to inherit its options.
              </p>
              <FieldError id="f-target-error" message={errors.sshTarget} />
            </div>

            <div className="field-row">
              <div className="field">
                <label className="field__label" htmlFor="f-bind">
                  Bind address {req}
                </label>
                {input("bindAddress", { placeholder: "127.0.0.1", mono: true })}
                <FieldError id="f-bind-error" message={errors.bindAddress} />
              </div>
              <div className="field">
                <label className="field__label" htmlFor="f-lport">
                  Local port {req}
                </label>
                {input("localPort", { placeholder: "5433", mono: true, numeric: true })}
                <FieldError id="f-lport-error" message={errors.localPort} />
              </div>
            </div>

            {needsRemote && (
              <div className="field-row">
                <div className="field">
                  <label className="field__label" htmlFor="f-rhost">
                    Remote host {req}
                  </label>
                  {input("remoteHost", { placeholder: "postgres.internal", mono: true })}
                  <FieldError id="f-rhost-error" message={errors.remoteHost} />
                </div>
                <div className="field">
                  <label className="field__label" htmlFor="f-rport">
                    Remote port {req}
                  </label>
                  {input("remotePort", { placeholder: "5432", mono: true, numeric: true })}
                  <FieldError id="f-rport-error" message={errors.remotePort} />
                </div>
              </div>
            )}

            <CommandPreview input={previewInput} />
          </section>

          <section className="section advanced">
            <button
              className="advanced__toggle"
              type="button"
              aria-expanded={advancedOpen}
              aria-controls="advanced-body"
              onClick={() => setAdvancedOpen((o) => !o)}
            >
              <Icon name="chevron-right" size={12} className="advanced__chevron" />
              Advanced — identity, jump host, retries
            </button>

            <div className="advanced__body" id="advanced-body" hidden={!advancedOpen}>
              <div className="field">
                <label className="field__label" htmlFor="f-identity">
                  Identity file <span className="field__optional">optional</span>
                </label>
                <input
                  className="input input--mono"
                  id="f-identity"
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="~/.ssh/id_ed25519"
                  value={identityFile}
                  onChange={(e) => setIdentityFile(e.target.value)}
                />
                <p className="field__hint">
                  Leave blank to let ssh use your agent and default keys.
                </p>
              </div>

              <div className="field">
                <label className="field__label" htmlFor="f-jump">
                  Jump host <span className="field__optional">ProxyJump, optional</span>
                </label>
                <input
                  className="input input--mono"
                  id="f-jump"
                  type="text"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder="jump-bastion  or  user@jump:2222"
                  value={proxyJump}
                  onChange={(e) => setProxyJump(e.target.value)}
                />
              </div>

              <div className="field">
                <label className="field__label" htmlFor="f-pass">
                  Key passphrase <span className="field__optional">stored in OS keychain</span>
                </label>
                <input
                  className="input"
                  id="f-pass"
                  type="password"
                  autoComplete="new-password"
                  placeholder={
                    hasStored
                      ? "Stored — leave blank to keep it"
                      : "Leave blank for no passphrase"
                  }
                  value={passphrase}
                  onChange={(e) => setPassphrase(e.target.value)}
                />
                {hasStored && initial && (
                  <p className="passphrase-note">
                    <Icon name="key" size={13} />
                    A passphrase is stored for this profile.
                    <button
                      className="btn btn--sm btn--danger-ghost"
                      type="button"
                      onClick={async () => {
                        try {
                          await deletePassphrase(initial.id);
                          setHasStored(false);
                          notify("Passphrase cleared from keychain", "success");
                        } catch (err) {
                          notify(`Could not clear passphrase: ${String(err)}`, "error");
                        }
                      }}
                    >
                      Clear
                    </button>
                  </p>
                )}
              </div>

              <div className="field-row">
                <div className="field">
                  <label className="field__label" htmlFor="f-keepalive">
                    Keep-alive
                  </label>
                  <div className="input-group">
                    {input("keepAlive", { mono: true, numeric: true })}
                    <span className="btn btn--ghost" aria-hidden="true" style={{ pointerEvents: "none" }}>
                      sec
                    </span>
                  </div>
                  <FieldError id="f-keepalive-error" message={errors.keepAlive} />
                </div>
                <div className="field">
                  <label className="field__label" htmlFor="f-retries">
                    Max retries
                  </label>
                  {input("maxRetries", { mono: true, numeric: true })}
                  <FieldError id="f-retries-error" message={errors.maxRetries} />
                </div>
              </div>

              <div className="field" style={{ maxWidth: 180 }}>
                <label className="field__label" htmlFor="f-delay">
                  Retry delay
                </label>
                <div className="input-group">
                  {input("retryDelay", { mono: true, numeric: true })}
                  <span className="btn btn--ghost" aria-hidden="true" style={{ pointerEvents: "none" }}>
                    sec
                  </span>
                </div>
                <FieldError id="f-delay-error" message={errors.retryDelay} />
              </div>

              <label className="switch-row" htmlFor="f-autostart">
                <span className="switch-row__body">
                  <span className="switch-row__title">Auto-start with app</span>
                  <span className="switch-row__desc">Bring this tunnel up when tnls launches.</span>
                </span>
                <input
                  className="switch"
                  id="f-autostart"
                  type="checkbox"
                  role="switch"
                  checked={autoStart}
                  onChange={(e) => setAutoStart(e.target.checked)}
                />
              </label>
            </div>
          </section>

          {saveError && (
            <p className="form-error" role="alert">
              <Icon name="alert" size={14} />
              <span>{saveError}</span>
            </p>
          )}
        </form>
      </div>

      <footer className="drawer__foot">
        <span className="drawer__foot-spacer" />
        <button className="btn btn--ghost" type="button" onClick={onClose}>
          Cancel
        </button>
        <button className="btn btn--primary" type="submit" form="tunnel-form" disabled={saving}>
          {saving ? "Saving…" : initial ? "Save changes" : "Create tunnel"}
        </button>
      </footer>
    </>
  );
}
