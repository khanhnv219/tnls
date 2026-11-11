import { useEffect, useState } from "react";

/**
 * Mount/unmount with an enter and exit transition.
 *
 * `mounted` stays true while the exit transition runs; `shown` flips to true
 * two frames after mounting so the CSS transition starts from the closed
 * state (`.is-open` is added after the element has been painted once).
 */
export function usePresence(open: boolean, exitMs: number) {
  const [mounted, setMounted] = useState(open);
  const [shown, setShown] = useState(false);

  useEffect(() => {
    if (open) {
      setMounted(true);
      let frame = requestAnimationFrame(() => {
        frame = requestAnimationFrame(() => setShown(true));
      });
      return () => cancelAnimationFrame(frame);
    }
    setShown(false);
    const timer = setTimeout(() => setMounted(false), exitMs);
    return () => clearTimeout(timer);
  }, [open, exitMs]);

  return { mounted, shown };
}
