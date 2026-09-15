import { invoke } from "@tauri-apps/api/core";
import { isNetworkPath } from "$lib/remote/network-locations.js";
import { isRemotePath } from "$lib/remote/remote-locations.js";
import type { Location } from "$lib/tabs/tabs.js";

/** Local folders can be favorites; files, buckets, shares and remote paths can't. */
export function canFavorite(entry: {
  is_directory: boolean;
  path: string;
  kind?: string;
}): boolean {
  return (
    entry.is_directory && !entry.kind && !isRemotePath(entry.path) && !isNetworkPath(entry.path)
  );
}

/** Insertion index for a drag over row `rowIndex`: before it in its upper half, after it in its lower half. */
export function dropIndex(
  clientY: number,
  rowTop: number,
  rowHeight: number,
  rowIndex: number,
): number {
  return clientY < rowTop + rowHeight / 2 ? rowIndex : rowIndex + 1;
}

/** Insertion index in the sidebar favorites under the viewport point, or `null` outside them. */
export function favoriteDropIndexAt(x: number, y: number): number | null {
  const element = document.elementFromPoint(x, y);
  const row = element?.closest<HTMLElement>("[data-favorite-index]");
  if (row) {
    const rect = row.getBoundingClientRect();
    return dropIndex(y, rect.top, rect.height, Number(row.dataset.favoriteIndex));
  }
  const list = element?.closest<HTMLElement>("[data-favorites-drop]");
  return list ? Number(list.dataset.favoritesDrop) : null;
}

/** Path of the sidebar favorite row under the viewport point, or `null`. */
export function favoritePathAt(x: number, y: number): string | null {
  return document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-favorite-path]")?.dataset.favoritePath ?? null;
}

/** Moves `from` to the insertion index `to`, which counts positions in the original list. */
export function moveTo<T>(items: T[], from: number, to: number): T[] {
  const result = [...items];
  const [item] = result.splice(from, 1);
  result.splice(from < to ? to - 1 : to, 0, item);
  return result;
}

export function fetchFavorites(): Promise<Location[]> {
  return invoke<Location[]>("favorites");
}

export function addFavorite(path: string, index?: number): Promise<Location[]> {
  return invoke<Location[]>("add_favorite", { path, index });
}

export function removeFavorite(path: string): Promise<Location[]> {
  return invoke<Location[]>("remove_favorite", { path });
}

export function reorderFavorites(paths: string[]): Promise<Location[]> {
  return invoke<Location[]>("reorder_favorites", { paths });
}
