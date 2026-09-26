import type { SortColumn } from "./sort.js";
export type ListColumn = SortColumn;
export const DEFAULT_COLUMNS: ListColumn[] = ["name", "type", "size", "date", "modified"];
export const COLUMN_LABELS: Record<ListColumn, string> = {
  name: "Name",
  type: "Type",
  size: "Size",
  date: "Created",
  modified: "Modified",
};
type StorageAccess = Pick<Storage, "getItem" | "setItem">;
const KEY = "folder-column-order";
const folderKey = (path: string) => path.replace(/[/\\]+$/, "") || path;
function read(storage: StorageAccess): Record<string, unknown> {
  try {
    const value = JSON.parse(storage.getItem(KEY) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value) ? value : {};
  } catch {
    return {};
  }
}
export function loadColumns(path: string, storage: StorageAccess = localStorage): ListColumn[] {
  const saved = read(storage)[folderKey(path)];
  if (!Array.isArray(saved)) return [...DEFAULT_COLUMNS];
  const valid = [
    ...new Set(saved.filter((column): column is ListColumn => DEFAULT_COLUMNS.includes(column))),
  ];
  return [...valid, ...DEFAULT_COLUMNS.filter((column) => !valid.includes(column))];
}
export function saveColumns(
  path: string,
  columns: ListColumn[],
  storage: StorageAccess = localStorage,
) {
  if (!path) return;
  const all = new Map(Object.entries(read(storage)));
  all.delete(folderKey(path));
  all.set(folderKey(path), columns);
  try {
    storage.setItem(KEY, JSON.stringify(Object.fromEntries([...all].slice(-500))));
  } catch {
    /* Keep the current visit usable. */
  }
}
export function moveColumn(columns: ListColumn[], from: ListColumn, to: ListColumn): ListColumn[] {
  const next = [...columns];
  const index = next.indexOf(from),
    target = next.indexOf(to);
  if (index < 0 || target < 0) return next;
  next.splice(index, 1);
  next.splice(target, 0, from);
  return next;
}
