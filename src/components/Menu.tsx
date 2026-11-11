import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon, type IconName } from "./Icon";

export type MenuEntry =
  | {
      kind: "item";
      key: string;
      label: string;
      icon?: IconName;
      shortcut?: string;
      danger?: boolean;
      disabled?: boolean;
      onSelect: () => void;
    }
  | { kind: "sep"; key: string }
  | { kind: "label"; key: string; label: string };

/**
 * Overflow menu anchored to a button: right-aligned under the anchor, flips
 * above it when there is no room below. Closes on outside press, Esc,
 * resize, and scroll. Arrow keys move between enabled items.
 */
export function Menu({
  anchor,
  entries,
  label,
  onClose,
}: {
  anchor: HTMLElement;
  entries: MenuEntry[];
  label: string;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const [open, setOpen] = useState(false);

  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const rect = anchor.getBoundingClientRect();
    const w = menu.offsetWidth;
    const h = menu.offsetHeight;
    let left = rect.right - w;
    let top = rect.bottom + 6;
    if (left < 8) left = 8;
    if (top + h > window.innerHeight - 8) top = rect.top - h - 6;
    setPos({ left: Math.round(left), top: Math.round(Math.max(8, top)) });
  }, [anchor]);

  useEffect(() => {
    if (!pos) return;
    const frame = requestAnimationFrame(() => setOpen(true));
    ref.current
      ?.querySelector<HTMLButtonElement>(".menu__item:not(:disabled)")
      ?.focus();
    return () => cancelAnimationFrame(frame);
  }, [pos]);

  useEffect(() => {
    const onPointer = (e: PointerEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor.contains(t)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        anchor.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const items = Array.from(
        ref.current?.querySelectorAll<HTMLButtonElement>(".menu__item:not(:disabled)") ?? [],
      );
      if (items.length === 0) return;
      e.preventDefault();
      e.stopPropagation();
      const idx = items.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        e.key === "ArrowDown"
          ? (idx + 1) % items.length
          : (idx - 1 + items.length) % items.length;
      items[next].focus();
    };
    const onDismiss = () => onClose();
    document.addEventListener("pointerdown", onPointer, true);
    // Capture on window so Esc closes the menu before any drawer handler.
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("resize", onDismiss);
    window.addEventListener("scroll", onDismiss, true);
    return () => {
      document.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("resize", onDismiss);
      window.removeEventListener("scroll", onDismiss, true);
    };
  }, [anchor, onClose]);

  return (
    <div
      ref={ref}
      className={`menu${open ? " is-open" : ""}`}
      role="menu"
      aria-label={label}
      style={
        pos
          ? { left: pos.left, top: pos.top }
          : { left: 0, top: 0, visibility: "hidden" }
      }
    >
      {entries.map((entry) => {
        if (entry.kind === "sep") return <div key={entry.key} className="menu__sep" />;
        if (entry.kind === "label") {
          return (
            <div key={entry.key} className="menu__label">
              {entry.label}
            </div>
          );
        }
        return (
          <button
            key={entry.key}
            type="button"
            role="menuitem"
            className={`menu__item${entry.danger ? " menu__item--danger" : ""}`}
            disabled={entry.disabled}
            onClick={() => {
              onClose();
              entry.onSelect();
            }}
          >
            {entry.icon && (
              <span className="menu__item-icon">
                <Icon name={entry.icon} size={14} />
              </span>
            )}
            <span>{entry.label}</span>
            {entry.shortcut && <span className="menu__shortcut">{entry.shortcut}</span>}
          </button>
        );
      })}
    </div>
  );
}
