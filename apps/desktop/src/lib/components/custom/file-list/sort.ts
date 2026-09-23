import type { DirectoryEntry } from "./list-item.svelte";

export type SortColumn = "name" | "type" | "size" | "date" | "modified";
export type SortDir = "asc" | "desc";

const nameCollator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Label for the Type column. */
export function entryType(entry: DirectoryEntry): string {
  if (entry.kind === "bucket") return "Bucket";
  if (entry.kind === "share") return "Share";
  return entry.is_directory ? "Folder" : "File";
}

function compare(a: DirectoryEntry, b: DirectoryEntry, column: SortColumn): number {
  switch (column) {
    case "name":
      return nameCollator.compare(a.name, b.name);
    case "type":
      return entryType(a).localeCompare(entryType(b));
    case "size":
      return (a.size ?? 0) - (b.size ?? 0);
    case "modified":
      return (a.modified ?? 0) - (b.modified ?? 0);
    case "date":
      return (a.created ?? 0) - (b.created ?? 0);
  }
}

export function sortEntries(
  entries: DirectoryEntry[],
  column: SortColumn,
  dir: SortDir,
): DirectoryEntry[] {
  const factor = dir === "asc" ? 1 : -1;
  return [...entries].sort((a, b) => {
    if (a.is_directory !== b.is_directory) return a.is_directory ? -1 : 1;
    return compare(a, b, column) * factor;
  });
}
