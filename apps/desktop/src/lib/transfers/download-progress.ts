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

/** A known total is required; unknown totals never become a fictitious 0%. */
export function downloadFraction(done: number, total: number): number | null {
  return Number.isFinite(done) && Number.isFinite(total) && done >= 0 && total > 0
    ? Math.min(1, done / total)
    : null;
}
