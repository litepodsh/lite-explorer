import type { SortColumn, SortDir } from "./sort.js";

export type FolderSort = { column: SortColumn; dir: SortDir };
type SortStorage = Pick<Storage, "getItem" | "setItem">;

const FOLDER_SORT_STORAGE = "folder-sort";
/** Oldest folders are forgotten past this many, so storage doesn't grow forever. */
export const FOLDER_SORT_LIMIT = 500;
const COLUMNS: SortColumn[] = ["name", "type", "size", "date"];

/** `/a/b/` and `/a/b` are the same folder; a root like `/` stays as is. */
function folderKey(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed && !trimmed.endsWith(":") ? trimmed : path;
}

function readAll(storage: SortStorage): Record<string, FolderSort> {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(FOLDER_SORT_STORAGE) ?? "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as Record<string, FolderSort>)
      : {};
  } catch {
    return {};
  }
}

/** The sort chosen last time in the folder at `path`, or `null` when there is none. */
export function loadFolderSort(
  path: string,
  storage: SortStorage = localStorage,
): FolderSort | null {
  if (!path) return null;
  const saved = readAll(storage)[folderKey(path)];
  if (!saved || !COLUMNS.includes(saved.column) || (saved.dir !== "asc" && saved.dir !== "desc"))
    return null;
  return { column: saved.column, dir: saved.dir };
}

/** Remembers the sort of the folder at `path`. Saving a folder again makes it the newest. */
export function saveFolderSort(
  path: string,
  sort: FolderSort,
  storage: SortStorage = localStorage,
) {
  if (!path) return;
  const key = folderKey(path);
  const all = readAll(storage);
  delete all[key];
  all[key] = sort;
  const keys = Object.keys(all);
  for (const old of keys.slice(0, Math.max(0, keys.length - FOLDER_SORT_LIMIT))) delete all[old];
  try {
    storage.setItem(FOLDER_SORT_STORAGE, JSON.stringify(all));
  } catch {
    /* storage full or unavailable: the sort still applies for this visit */
  }
}
