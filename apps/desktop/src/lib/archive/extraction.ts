import type { ArchiveEntry, ArchiveListing } from "$lib/file-ops/archive.js";
import { formatSize } from "$lib/components/custom/preview/format.js";

/** Paths for the tree: extractable entries only, directories ending with `/`. */
export function toTreePaths(entries: readonly ArchiveEntry[]): string[] {
  return entries
    .filter((entry) => !entry.skipped)
    .map((entry) => (entry.isDirectory ? `${entry.path}/` : entry.path));
}

export function fromTreePath(path: string): string {
  return path.replace(/\/+$/, "");
}

/** Archive paths to extract, without entries already covered by a selected folder. */
export function collapseSelection(paths: readonly string[]): string[] {
  const unique = [...new Set(paths.map(fromTreePath))].sort();
  return unique.filter(
    (path) => !unique.some((other) => other !== path && path.startsWith(`${other}/`)),
  );
}

export function sizeByTreePath(entries: readonly ArchiveEntry[]): Map<string, number> {
  return new Map(
    entries
      .filter((entry) => !entry.skipped && !entry.isDirectory)
      .map((entry) => [entry.path, entry.size]),
  );
}

export function summaryLine(listing: ArchiveListing): string {
  const kept = listing.entries.filter((entry) => !entry.skipped).length;
  const skipped = listing.entries.length - kept;
  const parts = [
    `${kept.toLocaleString("en-US")} ${kept === 1 ? "item" : "items"}`,
    `${formatSize(listing.uncompressedSize)} uncompressed`,
  ];
  if (skipped > 0) parts.push(`${skipped.toLocaleString("en-US")} skipped`);
  return parts.join(" · ");
}

export function dragLabel(paths: readonly string[]): string {
  if (paths.length !== 1) return `${paths.length} items`;
  return fromTreePath(paths[0]).split("/").pop() ?? paths[0];
}

export function dragIcon(paths: readonly string[]): "file" | "folder" {
  return paths.length === 1 && !paths[0].endsWith("/") ? "file" : "folder";
}
