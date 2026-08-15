/**
 * Pure formatting helpers shared by the UI kit and the wizard screens.
 * No i18n here on purpose: units are short international symbols (B/KB, ms/s/min/h) that read
 * fine in both zh-CN and en; localised prose belongs in the locale files.
 */

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** Drops a trailing ".0" so `1.0 MB` reads as `1 MB`. */
function trimZero(fixed: string): string {
  return fixed.endsWith(".0") ? fixed.slice(0, -2) : fixed;
}

/**
 * Formats a byte count using binary (1024) units: `0 B`, `512 B`, `1.5 KB`, `12.3 MB`.
 * Invalid or negative input renders as `0 B` so progress labels never show `NaN`.
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = unit === 0 ? String(Math.round(value)) : trimZero(value.toFixed(1));
  return `${text} ${BYTE_UNITS[unit]}`;
}

/**
 * Formats a duration given in milliseconds for humans:
 * `850 ms` · `2.4 s` · `1 min 05 s` · `1 h 02 min`.
 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0 ms";
  const wholeMs = Math.round(ms);
  if (wholeMs < 1000) return `${wholeMs} ms`;
  const totalSeconds = ms / 1000;
  if (totalSeconds < 60) return `${trimZero(totalSeconds.toFixed(1))} s`;
  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) {
    const seconds = Math.round(totalSeconds - totalMinutes * 60);
    return `${totalMinutes} min ${String(seconds).padStart(2, "0")} s`;
  }
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes - hours * 60;
  return `${hours} h ${String(minutes).padStart(2, "0")} min`;
}

/**
 * Formats a 0..1 ratio as an integer percentage (`0.42` → `42%`), clamped to 0..100.
 */
export function formatPercent(ratio: number): string {
  if (!Number.isFinite(ratio)) return "0%";
  const clamped = Math.min(1, Math.max(0, ratio));
  return `${Math.round(clamped * 100)}%`;
}

/**
 * Masks a secret for display, mirroring Rust `redact::mask_value`:
 * trimmed, then `***` for 8 chars or fewer, otherwise first 3 + `****` + last 4 chars.
 * Counts Unicode code points like the Rust side (`chars()`), not UTF-16 units.
 */
export function maskSecret(secret: string): string {
  const chars = Array.from(secret.trim());
  const n = chars.length;
  if (n === 0) return "";
  if (n <= 8) return "*".repeat(n);
  return `${chars.slice(0, 3).join("")}****${chars.slice(n - 4).join("")}`;
}
