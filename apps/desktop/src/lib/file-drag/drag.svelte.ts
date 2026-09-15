export type DraggedEntry = {
  path: string;
  name: string;
  paneId: string;
  isDirectory: boolean;
  kind?: string;
};

/** What the card following the pointer shows: the dragged item and what dropping would do. */
export type DragGhost = {
  x: number;
  y: number;
  name: string;
  icon: "folder" | "file" | "drive";
  action: "favorite" | "copy" | "move" | null;
};

class DragState {
  entry = $state<DraggedEntry | null>(null);
  tab = $state<{ paneId: string; id: string } | null>(null);
  /** Path of a favorite being reordered in the sidebar. */
  favorite = $state<string | null>(null);
  /** Where a folder or favorite dragged over the sidebar favorites would be inserted. */
  favoriteDropAt = $state<number | null>(null);
  /** Pane whose file list a dragged entry from another pane is over. */
  overPaneId = $state<string | null>(null);
  /** Path of the row in its own list a dragged entry is over. */
  overEntryPath = $state<string | null>(null);
  ghost = $state<DragGhost | null>(null);
  /** Drop handlers of the mounted file lists, by pane id. */
  readonly paneDrops = new Map<string, (path: string, options: { move: boolean }) => void>();
  /** Drop handler of the sidebar favorites. */
  favoritesDrop: ((path: string, index: number) => void) | null = null;
}

export const drag = new DragState();
