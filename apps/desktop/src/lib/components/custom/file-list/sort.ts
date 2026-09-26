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

function comparator(column: SortColumn, dir: SortDir) {
  const factor = dir === "asc" ? 1 : -1;
  return (a: DirectoryEntry, b: DirectoryEntry) => {
    if (a.is_directory !== b.is_directory) return a.is_directory ? -1 : 1;
    return compare(a, b, column) * factor;
  };
}

export function sortEntries(
  entries: DirectoryEntry[],
  column: SortColumn,
  dir: SortDir,
): DirectoryEntry[] {
  return [...entries].sort(comparator(column, dir));
}

/** The shown order of a listing and the listing it came from, kept to sort streamed rows. */
export type ListOrder = {
  source: DirectoryEntry[];
  shown: DirectoryEntry[];
  column: SortColumn | null;
  dir: SortDir;
  showHidden: boolean;
};

/** Filters and sorts `entries`. When they only add rows to the end of `previous.source`,
 *  just the new rows are sorted and merged in, so a streamed listing is not re-sorted whole
 *  for every batch. The result matches a full sort, stable order included. */
export function orderEntries(
  entries: DirectoryEntry[],
  column: SortColumn | null,
  dir: SortDir,
  showHidden: boolean,
  previous: ListOrder | null,
): ListOrder {
  const keep = (entry: DirectoryEntry) => showHidden || !entry.is_hidden;
  const order = (list: DirectoryEntry[]) => (column ? list.sort(comparator(column, dir)) : list);
  if (
    previous &&
    previous.column === column &&
    previous.dir === dir &&
    previous.showHidden === showHidden &&
    appends(previous.source, entries)
  ) {
    if (entries.length === previous.source.length) return previous;
    const added = order(entries.slice(previous.source.length).filter(keep));
    const shown = column
      ? merge(previous.shown, added, comparator(column, dir))
      : previous.shown.concat(added);
    return { source: entries, shown, column, dir, showHidden };
  }
  const kept = showHidden ? entries : entries.filter(keep);
  const shown = column ? order(kept === entries ? [...entries] : kept) : kept;
  return { source: entries, shown, column, dir, showHidden };
}

/** Whether `next` is `previous` with rows added at the end. */
function appends(previous: DirectoryEntry[], next: DirectoryEntry[]): boolean {
  if (previous === next) return true;
  if (next.length < previous.length) return false;
  for (let i = 0; i < previous.length; i++) if (previous[i] !== next[i]) return false;
  return true;
}

/** Merges two sorted lists. Ties keep `first` ahead, as a stable sort of first+second would. */
function merge(
  first: DirectoryEntry[],
  second: DirectoryEntry[],
  compare: (a: DirectoryEntry, b: DirectoryEntry) => number,
): DirectoryEntry[] {
  if (second.length === 0) return first;
  const merged: DirectoryEntry[] = [];
  let i = 0;
  let j = 0;
  while (i < first.length && j < second.length) {
    merged.push(compare(second[j], first[i]) < 0 ? second[j++] : first[i++]);
  }
  for (; i < first.length; i++) merged.push(first[i]);
  for (; j < second.length; j++) merged.push(second[j]);
  return merged;
}
