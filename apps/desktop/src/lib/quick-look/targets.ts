import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";

/** What Quick Look shows: the selection in listing order, starting at the focused row. */
export type QuickLookTargets = { paths: string[]; index: number };

/** Local files only: Quick Look reads from disk, so `s3://`, SFTP/FTP and SMB share rows
 *  (which use URL-like paths) are left out. Mounted shares are local paths and stay. */
export function quickLookTargets(selected: DirectoryEntry[], focusPath: string): QuickLookTargets {
  const paths = selected
    .map((entry) => entry.path)
    .filter((path) => path.startsWith("/"));
  return { paths, index: Math.max(0, paths.indexOf(focusPath)) };
}
