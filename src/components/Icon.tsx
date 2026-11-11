import type { CSSProperties } from "react";

export type IconName =
  | "search"
  | "x"
  | "plus"
  | "refresh"
  | "more"
  | "play"
  | "stop"
  | "restart"
  | "edit"
  | "copy"
  | "check"
  | "terminal"
  | "trash"
  | "download"
  | "upload"
  | "file-code"
  | "sun"
  | "moon"
  | "alert"
  | "chevron-right"
  | "arrow-down"
  | "dot"
  | "circle"
  | "loader"
  | "pause"
  | "layers"
  | "key";

/**
 * One `<svg class="od-icon">` instance of a sprite symbol. The size is always
 * set: without `--od-icon-size` the UA box is 300x150 (see `.od-icon`).
 */
export function Icon({
  name,
  size = 14,
  className,
  style,
}: {
  name: IconName;
  size?: number;
  className?: string;
  style?: CSSProperties;
}) {
  return (
    <svg
      className={className ? `od-icon ${className}` : "od-icon"}
      style={{ ["--od-icon-size" as string]: `${size}px`, ...style }}
      aria-hidden="true"
      focusable="false"
    >
      <use href={`#i-${name}`} />
    </svg>
  );
}

const STROKE = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/**
 * The design-system icon sprite: 24x24 grid, 1.6 stroke, round caps.
 * Copied from the OpenDesign export `index.html`. Render once at the root.
 */
export function IconSprite() {
  return (
    <svg className="icon-sprite" width="0" height="0" aria-hidden="true" focusable="false">
      <defs>
        <symbol id="i-search" viewBox="0 0 24 24" {...STROKE}>
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.4-3.4" />
        </symbol>
        <symbol id="i-x" viewBox="0 0 24 24" {...STROKE}>
          <path d="M18 6 6 18M6 6l12 12" />
        </symbol>
        <symbol id="i-plus" viewBox="0 0 24 24" {...STROKE} strokeWidth={1.8}>
          <path d="M12 5v14M5 12h14" />
        </symbol>
        <symbol id="i-refresh" viewBox="0 0 24 24" {...STROKE}>
          <path d="M21 12a9 9 0 1 1-3-6.7" />
          <path d="M21 4v5h-5" />
        </symbol>
        <symbol id="i-more" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="6" cy="12" r="1.6" />
          <circle cx="12" cy="12" r="1.6" />
          <circle cx="18" cy="12" r="1.6" />
        </symbol>
        <symbol id="i-play" viewBox="0 0 24 24" fill="currentColor">
          <path d="M8 5.6c0-.9 1-1.5 1.8-1L18.4 11c.8.5.8 1.6 0 2.1L9.8 19.4c-.8.5-1.8-.1-1.8-1z" />
        </symbol>
        <symbol id="i-stop" viewBox="0 0 24 24" fill="currentColor">
          <rect x="6.5" y="6.5" width="11" height="11" rx="1.6" />
        </symbol>
        <symbol id="i-restart" viewBox="0 0 24 24" {...STROKE}>
          <path d="M3 12a9 9 0 0 1 15.5-6.2" />
          <path d="M21 12a9 9 0 0 1-15.5 6.2" />
          <path d="M18 3v4h-4M6 21v-4h4" />
        </symbol>
        <symbol id="i-edit" viewBox="0 0 24 24" {...STROKE}>
          <path d="M4 20h4l10-10a2.8 2.8 0 0 0-4-4L4 16z" />
          <path d="m14 6 4 4" />
        </symbol>
        <symbol id="i-copy" viewBox="0 0 24 24" {...STROKE}>
          <rect x="9" y="9" width="11" height="11" rx="2" />
          <path d="M15 6.5V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h.5" />
        </symbol>
        <symbol id="i-check" viewBox="0 0 24 24" {...STROKE} strokeWidth={2}>
          <path d="m5 12.5 4.5 4.5L19 7" />
        </symbol>
        <symbol id="i-terminal" viewBox="0 0 24 24" {...STROKE}>
          <rect x="3" y="4" width="18" height="16" rx="2.5" />
          <path d="m7 10 2.5 2L7 14M12.5 14.5H16" />
        </symbol>
        <symbol id="i-trash" viewBox="0 0 24 24" {...STROKE}>
          <path d="M4 7h16M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7M6 7l.8 12a2 2 0 0 0 2 1.9h6.4a2 2 0 0 0 2-1.9L18 7" />
          <path d="M10 11v6M14 11v6" />
        </symbol>
        <symbol id="i-download" viewBox="0 0 24 24" {...STROKE}>
          <path d="M12 4v11M8 11.5l4 4 4-4" />
          <path d="M4.5 19.5h15" />
        </symbol>
        <symbol id="i-upload" viewBox="0 0 24 24" {...STROKE}>
          <path d="M12 19V8M8 11.5l4-4 4 4" />
          <path d="M4.5 4.5h15" />
        </symbol>
        <symbol id="i-file-code" viewBox="0 0 24 24" {...STROKE}>
          <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
          <path d="M14 3v5h5" />
          <path d="m10.5 12.5-1.8 2 1.8 2M13.5 12.5l1.8 2-1.8 2" />
        </symbol>
        <symbol id="i-sun" viewBox="0 0 24 24" {...STROKE}>
          <circle cx="12" cy="12" r="4" />
          <path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.2 5.2l1.4 1.4M17.4 17.4l1.4 1.4M18.8 5.2l-1.4 1.4M6.6 17.4l-1.4 1.4" />
        </symbol>
        <symbol id="i-moon" viewBox="0 0 24 24" {...STROKE}>
          <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5" />
        </symbol>
        <symbol id="i-alert" viewBox="0 0 24 24" {...STROKE} strokeWidth={1.7}>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 7.5v5.5M12 16.3v.2" />
        </symbol>
        <symbol id="i-chevron-right" viewBox="0 0 24 24" {...STROKE} strokeWidth={1.8}>
          <path d="m9.5 6 6 6-6 6" />
        </symbol>
        <symbol id="i-arrow-down" viewBox="0 0 24 24" {...STROKE} strokeWidth={1.8}>
          <path d="M12 5v14M6.5 13.5 12 19l5.5-5.5" />
        </symbol>
        <symbol id="i-dot" viewBox="0 0 24 24" fill="currentColor">
          <circle cx="12" cy="12" r="4.5" />
        </symbol>
        <symbol id="i-circle" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.7}>
          <circle cx="12" cy="12" r="8" />
        </symbol>
        <symbol id="i-loader" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
          <path d="M12 3.5a8.5 8.5 0 1 0 8.5 8.5" />
        </symbol>
        <symbol id="i-pause" viewBox="0 0 24 24" fill="currentColor">
          <rect x="7" y="5.5" width="3.6" height="13" rx="1.2" />
          <rect x="13.4" y="5.5" width="3.6" height="13" rx="1.2" />
        </symbol>
        <symbol id="i-layers" viewBox="0 0 24 24" {...STROKE}>
          <path d="m12 3 8.5 4.5L12 12 3.5 7.5z" />
          <path d="m3.5 12.5 8.5 4.5 8.5-4.5" />
          <path d="m3.5 17 8.5 4.5 8.5-4.5" />
        </symbol>
        <symbol id="i-key" viewBox="0 0 24 24" {...STROKE}>
          <circle cx="8" cy="15.5" r="3.5" />
          <path d="m10.6 13 7-7M16 5.5 19 8.5M14.5 7 17 9.5" />
        </symbol>
      </defs>
    </svg>
  );
}
