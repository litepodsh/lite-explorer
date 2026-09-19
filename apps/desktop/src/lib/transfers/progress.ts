/** Done/total as a clamped 0..1 fraction, or null when the total is unknown. */
export function progressFraction(done: number, total: number): number | null {
  if (total <= 0) return null;
  return Math.min(1, Math.max(0, done / total));
}

/** Remaining seconds from the observed rate, or null when there is not enough signal. */
export function etaSeconds(done: number, total: number, elapsedSeconds: number): number | null {
  if (total <= 0 || done <= 0 || elapsedSeconds <= 0 || done >= total) return null;
  const rate = done / elapsedSeconds;
  if (rate <= 0) return null;
  return Math.max(0, (total - done) / rate);
}

export function formatEta(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds)) return null;
  if (seconds < 60) return `${Math.ceil(seconds)}s left`;
  return `${Math.ceil(seconds / 60)}m left`;
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/** Human-readable byte size, e.g. `512 B`, `1.4 MB`, `2.0 GB`. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exponent = Math.min(BYTE_UNITS.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const value = bytes / 1024 ** exponent;
  const decimals = exponent === 0 || value >= 100 ? 0 : 1;
  return `${value.toFixed(decimals)} ${BYTE_UNITS[exponent]}`;
}
