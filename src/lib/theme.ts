export type Theme = "dark" | "light";

const STORAGE_KEY = "tnls-theme";

/** Saved choice first, then the OS preference. Dark is the primary theme. */
export function initialTheme(): Theme {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "dark" || saved === "light") return saved;
  } catch {
    // Storage blocked: fall through to the OS preference.
  }
  return window.matchMedia("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // The toggle still works for this session.
  }
}
