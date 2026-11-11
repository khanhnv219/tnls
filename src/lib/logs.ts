export type LogLevel = "ERROR" | "WARN" | "INFO" | "DEBUG";

/**
 * Best-effort level for a raw ssh stderr line. The backend stores lines as
 * plain text, so the level is inferred from OpenSSH's own wording.
 */
export function classifyLevel(line: string): LogLevel {
  const l = line.trim().toLowerCase();
  if (/^debug\d?:/.test(l)) return "DEBUG";
  if (/^warning:/.test(l) || /\bwarn(ing)?\b/.test(l)) return "WARN";
  if (
    /^(ssh|fatal|error):/.test(l) ||
    /\b(error|failed|failure|denied|refused|timed out|unreachable|no route|could not|cannot|broken pipe|closed by remote|host key verification)\b/.test(
      l,
    )
  ) {
    return "ERROR";
  }
  return "INFO";
}
