import type { FolderUsageEntry } from "./types.js";

/** Entering the overview rescans the home folder once the last scan is this old. */
export const STALE_AFTER_SECONDS = 30 * 60;

export function isStale(scannedAt: number | null, nowSeconds: number): boolean {
  return scannedAt === null || nowSeconds - scannedAt >= STALE_AFTER_SECONDS;
}

export function formatAge(scannedAt: number, nowSeconds: number): string {
  const seconds = Math.max(0, nowSeconds - scannedAt);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} d ago`;
}

export type DiskBreakdown = { used: number; home: number; system: number; free: number };

/** Splits a volume into your home folder, everything else that is used, and free space. */
export function diskBreakdown(
  volume: { total_bytes: number; free_bytes: number },
  homeBytes: number | null,
): DiskBreakdown {
  const free = Math.min(volume.free_bytes, volume.total_bytes);
  const used = volume.total_bytes - free;
  const home = Math.min(Math.max(homeBytes ?? 0, 0), used);
  return { used, home, system: used - home, free };
}

export type FolderSummary = {
  top: FolderUsageEntry[];
  restBytes: number;
  /** True when hidden folders were folded into the rest because they are not shown. */
  restHasHidden: boolean;
};

export function summarizeFolders(
  entries: FolderUsageEntry[],
  totalBytes: number,
  options: { limit?: number; showHidden: boolean },
): FolderSummary {
  const limit = options.limit ?? 6;
  const top = entries
    .filter((entry) => options.showHidden || !entry.is_hidden)
    .sort((a, b) => b.bytes - a.bytes)
    .slice(0, limit);
  const shown = top.reduce((sum, entry) => sum + entry.bytes, 0);
  return {
    top,
    restBytes: Math.max(0, totalBytes - shown),
    restHasHidden: !options.showHidden && entries.some((entry) => entry.is_hidden),
  };
}

/** Puts a freshly measured folder into the list, replacing its older size. */
export function upsertEntry(
  entries: FolderUsageEntry[],
  entry: FolderUsageEntry,
): FolderUsageEntry[] {
  return [...entries.filter((item) => item.path !== entry.path), entry].sort(
    (a, b) => b.bytes - a.bytes,
  );
}
