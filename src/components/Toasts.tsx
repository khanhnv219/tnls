import { useCallback, useEffect, useRef, useState } from "react";
import { Icon, type IconName } from "./Icon";

export type ToastTone = "success" | "error" | "info" | "warn";
export type PushToast = (message: string, tone?: ToastTone, ms?: number) => void;

interface ToastItem {
  id: number;
  message: string;
  tone: ToastTone;
  leaving: boolean;
}

const GLYPH: Record<ToastTone, IconName> = {
  success: "check",
  error: "alert",
  info: "dot",
  warn: "alert",
};

/** Exit runs at ~60% of the 180ms entrance (see DESIGN_SYSTEM.md §7). */
const EXIT_MS = 220;

export function useToasts(): { items: ToastItem[]; push: PushToast } {
  const [items, setItems] = useState<ToastItem[]>([]);
  const seq = useRef(0);

  const push = useCallback<PushToast>((message, tone = "info", ms) => {
    const id = ++seq.current;
    // Errors stay longer: they usually need to be read, not glanced at.
    const life = ms ?? (tone === "error" ? 6000 : 3200);
    setItems((cur) => [...cur, { id, message, tone, leaving: false }]);
    setTimeout(() => {
      setItems((cur) => cur.map((t) => (t.id === id ? { ...t, leaving: true } : t)));
      setTimeout(() => setItems((cur) => cur.filter((t) => t.id !== id)), EXIT_MS);
    }, life);
  }, []);

  return { items, push };
}

function Toast({ item }: { item: ToastItem }) {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => setOpen(true));
    });
    return () => cancelAnimationFrame(frame);
  }, []);
  const cls = ["toast", open ? "is-open" : "", item.leaving ? "is-leaving" : ""]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls} data-tone={item.tone}>
      <span className="toast__icon">
        <Icon name={GLYPH[item.tone]} size={15} />
      </span>
      <span className="toast__msg">{item.message}</span>
    </div>
  );
}

export function Toasts({ items }: { items: ToastItem[] }) {
  return (
    <div className="toasts" role="status" aria-live="polite">
      {items.map((t) => (
        <Toast key={t.id} item={t} />
      ))}
    </div>
  );
}
