import { favoriteDropIndexAt } from "$lib/favorites/favorites.js";

export type DropTarget =
  | { kind: "favorites"; index: number }
  | { kind: "pane"; paneId: string }
  | { kind: "entry"; paneId: string; path: string };

/** What is under the viewport point during a drag: the sidebar favorites, a file list row, or a file list. */
export function dropTargetAt(x: number, y: number): DropTarget | null {
  const index = favoriteDropIndexAt(x, y);
  if (index !== null) return { kind: "favorites", index };
  const element = document.elementFromPoint(x, y);
  const paneId = element?.closest<HTMLElement>("[data-file-list]")?.dataset.paneId;
  if (!paneId) return null;
  const path = element?.closest<HTMLElement>("[data-entry-path]")?.dataset.entryPath;
  return path ? { kind: "entry", paneId, path } : { kind: "pane", paneId };
}
