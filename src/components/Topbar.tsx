import { useRef, useState, type RefObject } from "react";
import { Icon } from "./Icon";
import { Menu, type MenuEntry } from "./Menu";
import type { Theme } from "../lib/theme";

export function Topbar({
  total,
  running,
  failed,
  query,
  onQuery,
  searchRef,
  theme,
  onToggleTheme,
  moreEntries,
  onNew,
}: {
  total: number;
  running: number;
  failed: number;
  query: string;
  onQuery: (q: string) => void;
  searchRef: RefObject<HTMLInputElement | null>;
  theme: Theme;
  onToggleTheme: () => void;
  moreEntries: MenuEntry[];
  onNew: () => void;
}) {
  const moreRef = useRef<HTMLButtonElement>(null);
  const [moreOpen, setMoreOpen] = useState(false);

  return (
    <header className="topbar">
      <div className="brand">
        <h1 className="brand__mark">tnls</h1>
      </div>

      <div className="stats" aria-live="polite">
        <span className="stat-pill">
          <span className="stat-pill__num">{total}</span>
          <span>{total === 1 ? "tunnel" : "tunnels"}</span>
        </span>
        <span className="stat-pill stat-pill--live">
          <span className="stat-pill__dot" aria-hidden="true" />
          <span className="stat-pill__num">{running}</span>
          <span>running</span>
        </span>
        {failed > 0 && (
          <span className="stat-pill">
            <span className="stat-pill__num">{failed}</span>
            <span>failed</span>
          </span>
        )}
      </div>

      <div className="topbar__spacer" />

      <div className="topbar__actions">
        <div className="search">
          <Icon name="search" size={14} className="search__icon" />
          <input
            ref={searchRef}
            className="search__input"
            type="text"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
            placeholder="Search tunnels"
            aria-label="Search tunnels by name, host, or port"
            autoComplete="off"
            spellCheck={false}
          />
          {query === "" ? (
            <kbd className="search__kbd">/</kbd>
          ) : (
            <button
              className="search__clear"
              type="button"
              aria-label="Clear search"
              onClick={() => {
                onQuery("");
                searchRef.current?.focus();
              }}
            >
              <Icon name="x" size={12} />
            </button>
          )}
        </div>

        <button
          className="btn btn--icon"
          type="button"
          onClick={onToggleTheme}
          aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          title="Toggle theme"
        >
          <Icon name={theme === "dark" ? "sun" : "moon"} size={16} />
        </button>

        <button
          ref={moreRef}
          className="btn btn--icon"
          type="button"
          aria-label="More actions"
          aria-haspopup="menu"
          aria-expanded={moreOpen}
          title="Import, export, settings"
          onClick={() => setMoreOpen((o) => !o)}
        >
          <Icon name="more" size={16} />
        </button>

        <button className="btn btn--primary" type="button" onClick={onNew}>
          <Icon name="plus" size={15} />
          New tunnel
        </button>
      </div>

      {moreOpen && moreRef.current && (
        <Menu
          anchor={moreRef.current}
          entries={moreEntries}
          label="More actions"
          onClose={() => setMoreOpen(false)}
        />
      )}
    </header>
  );
}
