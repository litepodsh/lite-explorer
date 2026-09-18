import type { DirectoryEntry } from "$lib/components/custom/file-list/index.js";

/** Folder sizes are measured on demand and live only in the listing, so a re-read has to keep them. */
export function mergeListing(previous: DirectoryEntry[], next: DirectoryEntry[]): DirectoryEntry[] {
  if (previous.length === 0) return next;
  const measured = new Map<string, Pick<DirectoryEntry, "size" | "sizeComplete">>();
  for (const entry of previous) {
    if (entry.size != null || entry.sizeComplete != null) {
      measured.set(entry.path, { size: entry.size, sizeComplete: entry.sizeComplete });
    }
  }
  if (measured.size === 0) return next;
  return next.map((entry) => {
    const kept = measured.get(entry.path);
    return kept ? { ...entry, ...kept } : entry;
  });
}

/** Drops entries by path, for removals the UI already knows about. */
export function removePaths(entries: DirectoryEntry[], paths: Iterable<string>): DirectoryEntry[] {
  const gone = paths instanceof Set ? paths : new Set(paths);
  if (gone.size === 0) return entries;
  return entries.filter((entry) => !gone.has(entry.path));
}

/** Adds an entry, or replaces the listed copy of the same path. */
export function upsertEntry(entries: DirectoryEntry[], entry: DirectoryEntry): DirectoryEntry[] {
  const index = entries.findIndex((listed) => listed.path === entry.path);
  if (index === -1) return [...entries, entry];
  const next = [...entries];
  next[index] = entry;
  return next;
}

/** Applies a rename to a listed entry, keeping its measured size. Unknown paths are ignored. */
export function renameEntry(
  entries: DirectoryEntry[],
  fromPath: string,
  toPath: string,
  name: string,
): DirectoryEntry[] {
  const index = entries.findIndex((entry) => entry.path === fromPath);
  if (index === -1) return entries;
  const next = [...entries];
  next[index] = { ...next[index], name, path: toPath };
  return next;
}
