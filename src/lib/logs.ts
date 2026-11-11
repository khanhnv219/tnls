export type LogLevel = "ERROR" | "WARN" | "INFO" | "DEBUG";

/** One ssh stderr line as stored by the backend (`tnls_core::logs::LogLine`). */
export interface LogLine {
  /** ISO timestamp of when the supervisor collected the line. */
  at: string;
  level: LogLevel;
  text: string;
}

/** Local wall-clock `HH:MM:SS` for the log time column. */
export function clockTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "--:--:--";
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}
