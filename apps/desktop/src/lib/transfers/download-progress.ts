export type DownloadSnapshot = {
  path: string;
  fraction: number | null;
  bytesDone: number | null;
  missing: boolean;
};

export type FileProgress = {
  path: string;
  bytesDone: number;
  bytesTotal: number;
};

export function isPartialDownload(name: string): boolean {
  return /\.(download|crdownload|part)$/i.test(name);
}

/** Name without the in-progress suffix (`.download`, `.crdownload`, `.part`). If the suffix
 *  isn't there, returns the name unchanged. Used to match a partial file against the active
 *  transfer for the same logical file (Chrome writes `foo.zip.crdownload` for `foo.zip`). */
export function stripPartialSuffix(name: string): string {
  return name.replace(/\.(download|crdownload|part)$/i, "");
}

/** A known total is required; unknown totals never become a fictitious 0%. */
export function downloadFraction(done: number, total: number): number | null {
  return Number.isFinite(done) && Number.isFinite(total) && done >= 0 && total > 0
    ? Math.min(1, done / total)
    : null;
}
