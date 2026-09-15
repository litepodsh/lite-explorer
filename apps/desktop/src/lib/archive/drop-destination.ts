import { dropTargetAt } from "$lib/file-drag/drop-target.js";
import { favoritePathAt } from "$lib/favorites/favorites.js";

/** What a mounted pane tells drags about where they can extract. */
export type PaneFolder = {
  /** The pane's current folder when it is local, otherwise `null`. */
  folder: () => string | null;
  /** True when `path` is a local folder row in the pane. */
  isFolder: (path: string) => boolean;
};

export type DropCandidate =
  | { kind: "favorite"; path: string }
  | { kind: "pane"; paneId: string }
  | { kind: "entry"; paneId: string; path: string };

export function resolveDestination(candidate: DropCandidate | null, panes: ReadonlyMap<string, PaneFolder>): string | null {
  if (!candidate) return null;
  if (candidate.kind === "favorite") return candidate.path;
  const pane = panes.get(candidate.paneId);
  if (!pane) return null;
  if (candidate.kind === "entry" && pane.isFolder(candidate.path)) return candidate.path;
  return pane.folder();
}

/** Favorite row, file list row or file list under the viewport point. */
export function dropCandidateAt(x: number, y: number): DropCandidate | null {
  const favorite = favoritePathAt(x, y);
  if (favorite) return { kind: "favorite", path: favorite };
  const target = dropTargetAt(x, y);
  return target && target.kind !== "favorites" ? target : null;
}
