import { useEffect, useRef, type ReactNode } from "react";
import { usePresence } from "../lib/presence";

/**
 * Right-edge drawer with a scrim. Children are the drawer's grid rows
 * (head, scrolling body, foot). When `open` turns false the last children
 * stay rendered until the slide-out finishes, and focus returns to the
 * element that had it when the drawer opened.
 */
export function Drawer({
  open,
  wide,
  labelledBy,
  onClose,
  children,
}: {
  open: boolean;
  wide?: boolean;
  labelledBy: string;
  onClose: () => void;
  children: ReactNode;
}) {
  const { mounted, shown } = usePresence(open, 240);
  const last = useRef<ReactNode>(children);
  if (open) last.current = children;
  const returnFocus = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (open) {
      returnFocus.current = document.activeElement as HTMLElement | null;
      return;
    }
    const el = returnFocus.current;
    returnFocus.current = null;
    if (el && document.contains(el)) el.focus();
  }, [open]);

  if (!mounted) return null;

  return (
    <>
      <div className={`scrim${shown ? " is-open" : ""}`} onClick={onClose} />
      <aside
        className={`drawer${wide ? " drawer--wide" : ""}${shown ? " is-open" : ""}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelledBy}
      >
        {open ? children : last.current}
      </aside>
    </>
  );
}
