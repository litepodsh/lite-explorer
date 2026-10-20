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
  return (a: DirectoryEntry, b: DirectoryEntry) => compare(a, b, column) * factor;
}

export function sortEntries(
  entries: DirectoryEntry[],
  column: SortColumn,
  dir: SortDir,
): DirectoryEntry[] {
  return [...entries].sort(comparator(column, dir));
}

/** The shown order of a listing and the listing it came from, kept to sort streamed rows.
 *  `pinned` is the active-download set used to build `shown`; carrying it lets the incremental
 *  path detect when the active set changed (a download started or finished) so the top of the
 *  list rebuilds correctly. */
export type ListOrder = {
  source: DirectoryEntry[];
  shown: DirectoryEntry[];
  column: SortColumn | null;
  dir: SortDir;
  showHidden: boolean;
  /** Active-download set used when this order was built. */
  pinned: ReadonlySet<string>;
};

/** Filters and sorts `entries`. When they only add rows to the end of `previous.source`,
 *  just the new rows are sorted and merged in, so a streamed listing is not re-sorted whole
 *  for every batch. The result matches a full sort, stable order included.
 *
 *  "Pinned" entries are floated to the very top, in their original relative order, regardless
 *  of the active sort. The remaining entries are sorted as usual. Pinned entries are the union
 *  of three sets, all keyed by path:
 *    - `activeDownloads`: paths with a live transfer (jobs in `fileDownloads`).
 *    - `partialDownloads`: paths that look like in-progress browser downloads by extension
 *      (`.download`, `.crdownload`, `.part`). Detected by name; the caller passes the set.
 *    - `staleDownloads`: partial files whose base name is not in any active transfer — orphans
 *      from another app, or transfers that died. Still pinned so the user can resume or delete.
 *
 *  Carrying the union on `ListOrder.pinned` lets the incremental path detect any change
 *  (a transfer starting, finishing, a `.crdownload` arriving or being renamed away)
 *  and rebuild the top correctly without bookkeeping the pinned items themselves. */
export function orderEntries(
  entries: DirectoryEntry[],
  column: SortColumn | null,
  dir: SortDir,
  showHidden: boolean,
  previous: ListOrder | null,
  activeDownloads?: ReadonlySet<string>,
  partialDownloads?: ReadonlySet<string>,
  staleDownloads?: ReadonlySet<string>,
): ListOrder {
  const keep = (entry: DirectoryEntry) => showHidden || !entry.is_hidden;
  const order = (list: DirectoryEntry[]) => (column ? list.sort(comparator(column, dir)) : list);
  const pinSet = union3(activeDownloads, partialDownloads, staleDownloads);
  const split = (list: DirectoryEntry[]): { pinned: DirectoryEntry[]; rest: DirectoryEntry[] } => {
    if (pinSet.size === 0) return { pinned: [], rest: list };
    const pinned: DirectoryEntry[] = [];
    const rest: DirectoryEntry[] = [];
    for (const entry of list) (pinSet.has(entry.path) ? pinned : rest).push(entry);
    return { pinned, rest };
  };
  const assemble = (pinned: DirectoryEntry[], sorted: DirectoryEntry[]): DirectoryEntry[] =>
    pinned.length > 0 ? pinned.concat(sorted) : sorted;
  if (
    previous &&
    previous.column === column &&
    previous.dir === dir &&
    previous.showHidden === showHidden &&
    appends(previous.source, entries)
  ) {
    if (entries.length === previous.source.length) return previous;
    const added = entries.slice(previous.source.length).filter(keep);
    const { pinned: addedPinned, rest: addedRest } = split(added);
    const { rest: prevRest, pinned: prevPinned } = split(previous.shown);
    // Rebuild when the pinned block changed: a transfer started or finished, a partial file
    // arrived or got renamed away. Otherwise keep the cheap merge path.
    const pinningChanged = !setEq(previous.pinned, pinSet);
    if (pinningChanged || addedPinned.length > 0) {
      const all = showHidden ? entries : entries.filter(keep);
      const { pinned, rest } = split(all);
      const sorted = column ? order(rest) : rest;
      const shown = assemble(pinned, sorted);
      return { source: entries, shown, column, dir, showHidden, pinned: pinSet };
    }
    const sortedAdded = column ? order(addedRest) : addedRest;
    const merged = column ? merge(prevRest, sortedAdded, comparator(column, dir)) : prevRest.concat(sortedAdded);
    const shown = assemble(prevPinned, merged);
    return { source: entries, shown, column, dir, showHidden, pinned: previous.pinned };
  }
  const kept = showHidden ? entries : entries.filter(keep);
  const { pinned, rest } = split(kept === entries ? [...entries] : kept);
  const sorted = column ? order(rest) : rest;
  const shown = assemble(pinned, sorted);
  return { source: entries, shown, column, dir, showHidden, pinned: pinSet };
}

const EMPTY_SET: ReadonlySet<string> = new Set();

function setEq(a: ReadonlySet<string>, b: ReadonlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const v of a) if (!b.has(v)) return false;
  return true;
}

/** Three-way set union. Returns the shared `EMPTY_SET` when every side is empty/undefined so
 *  the hot path stays allocation-free. */
function union3(
  a?: ReadonlySet<string>,
  b?: ReadonlySet<string>,
  c?: ReadonlySet<string>,
): ReadonlySet<string> {
  const aEmpty = !a || a.size === 0;
  const bEmpty = !b || b.size === 0;
  const cEmpty = !c || c.size === 0;
  if (aEmpty && bEmpty && cEmpty) return EMPTY_SET;
  const merged = new Set<string>();
  if (!aEmpty) for (const v of a!) merged.add(v);
  if (!bEmpty) for (const v of b!) merged.add(v);
  if (!cEmpty) for (const v of c!) merged.add(v);
  return merged;
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
