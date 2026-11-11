import { useEffect, useState } from "react";
import { previewSshCommand, type CommandPreview as Preview } from "../lib/api";
import type { TunnelInput } from "../lib/types";
import { Icon } from "./Icon";

type Tok = "tok-flag" | "tok-addr" | "tok-opt" | "tok-target";

/** Flags whose following argument is the forward spec. */
const FORWARD_FLAGS = new Set(["-L", "-R", "-D"]);
/** Flags whose following argument is a plain option value. */
const VALUE_FLAGS = new Set(["-o", "-i", "-J"]);

function classify(args: string[]): Tok[] {
  return args.map((arg, i) => {
    const prev = args[i - 1];
    if (i === args.length - 1) return "tok-target";
    if (prev && FORWARD_FLAGS.has(prev)) return "tok-addr";
    if (prev && VALUE_FLAGS.has(prev)) return "tok-opt";
    if (FORWARD_FLAGS.has(arg) || arg === "-J" || arg === "-i") return "tok-flag";
    return "tok-opt";
  });
}

/** Renders `<placeholder>` parts of an argument in the placeholder colour. */
function Arg({ text, tok }: { text: string; tok: Tok }) {
  const parts = text.split(/(<[^>]+>)/).filter(Boolean);
  return (
    <>
      {parts.map((p, i) => (
        <span key={i} className={p.startsWith("<") && p.endsWith(">") ? "tok-ph" : tok}>
          {p}
        </span>
      ))}
    </>
  );
}

/**
 * Live read-out of the ssh invocation the form describes. The argv comes
 * from the backend (`preview_ssh_command`), built by the same code that
 * spawns ssh, so the preview cannot drift from what actually runs.
 */
export function CommandPreview({ input }: { input: TunnelInput }) {
  const [preview, setPreview] = useState<Preview | null>(null);
  const [failed, setFailed] = useState(false);
  const key = JSON.stringify(input);

  useEffect(() => {
    let stale = false;
    // Debounce typing; drop responses that arrive after a newer request.
    const t = setTimeout(() => {
      previewSshCommand(JSON.parse(key) as TunnelInput).then(
        (p) => {
          if (stale) return;
          setPreview(p);
          setFailed(false);
        },
        () => {
          if (!stale) setFailed(true);
        },
      );
    }, 120);
    return () => {
      stale = true;
      clearTimeout(t);
    };
  }, [key]);

  const toks = preview ? classify(preview.args) : [];

  return (
    <div className="preview">
      <div className="preview__head">
        <Icon name="terminal" size={13} style={{ color: "var(--text-3)" }} />
        <span className="preview__label">Generated command</span>
        <div className="preview__dots" aria-hidden="true">
          <span className="preview__dot" />
          <span className="preview__dot" />
          <span className="preview__dot" />
        </div>
      </div>
      <code className="preview__code" aria-live="polite">
        {failed && !preview ? (
          <span className="tok-ph">Preview unavailable</span>
        ) : preview ? (
          <>
            <span className="tok-flag" title={preview.program}>
              ssh
            </span>
            {preview.args.map((arg, i) => (
              <span key={i}>
                {" "}
                <Arg text={arg} tok={toks[i]} />
              </span>
            ))}
          </>
        ) : (
          <span className="tok-ph">…</span>
        )}
      </code>
    </div>
  );
}
