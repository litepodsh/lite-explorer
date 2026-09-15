/**
 * Multiple selection over an ordered list of paths, with Windows Explorer rules.
 *
 * `anchor` is where Shift ranges start; `focus` is the item the keyboard moves from. With the
 * primary modifier (Ctrl, Cmd on macOS) the focus can move without changing what is selected.
 */
export type Selection = { paths: string[]; anchor: string; focus: string };

export const EMPTY_SELECTION: Selection = { paths: [], anchor: "", focus: "" };

export type NavKey = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "PageUp" | "PageDown" | "Home" | "End";

export function isNavKey(key: string): key is NavKey {
  return ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "PageUp", "PageDown", "Home", "End"].includes(key);
}

export function selectOnly(path: string): Selection {
  return { paths: [path], anchor: path, focus: path };
}

export function toggle(selection: Selection, path: string): Selection {
  const paths = selection.paths.includes(path)
    ? selection.paths.filter((candidate) => candidate !== path)
    : [...selection.paths, path];
  return { paths, anchor: path, focus: path };
}

function range(order: string[], from: string, to: string): string[] {
  const end = order.indexOf(to);
  if (end < 0) return [];
  const start = order.indexOf(from);
  if (start < 0) return [to];
  return order.slice(Math.min(start, end), Math.max(start, end) + 1);
}

/** Shift+click: the range from the anchor to `path` replaces the selection. */
export function selectRange(selection: Selection, order: string[], path: string): Selection {
  const anchor = order.includes(selection.anchor) ? selection.anchor : path;
  return { paths: range(order, anchor, path), anchor, focus: path };
}

/** Ctrl+Shift+click: the range from the anchor to `path` is added to the selection. */
export function addRange(selection: Selection, order: string[], path: string): Selection {
  const anchor = order.includes(selection.anchor) ? selection.anchor : path;
  const added = new Set(range(order, anchor, path));
  const kept = new Set(selection.paths);
  return { paths: order.filter((candidate) => kept.has(candidate) || added.has(candidate)), anchor, focus: path };
}

export function selectAll(order: string[]): Selection {
  if (order.length === 0) return EMPTY_SELECTION;
  return { paths: [...order], anchor: order[0], focus: order[order.length - 1] };
}

/** Drops paths that are no longer listed, for example after a reload or delete. */
export function prune(selection: Selection, order: string[]): Selection {
  const listed = new Set(order);
  const paths = selection.paths.filter((path) => listed.has(path));
  const anchor = listed.has(selection.anchor) ? selection.anchor : (paths[0] ?? "");
  const focus = listed.has(selection.focus) ? selection.focus : (paths.at(-1) ?? "");
  if (paths.length === selection.paths.length && anchor === selection.anchor && focus === selection.focus) {
    return selection;
  }
  return { paths, anchor, focus };
}

/** Selected paths in list order. */
export function orderedPaths(selection: Selection, order: string[]): string[] {
  const selected = new Set(selection.paths);
  return order.filter((path) => selected.has(path));
}

export type NavLayout = {
  count: number;
  /** Items per row: 1 in the list view. */
  columns: number;
  /** Whole rows visible in the viewport. */
  pageRows: number;
  view: "list" | "grid";
};

/**
 * Index a navigation key moves to from `index` (-1 when nothing has focus), or null when the
 * key does not move in this view (Left/Right in the list view).
 */
export function navTarget(index: number, key: NavKey, layout: NavLayout): number | null {
  const { count, view } = layout;
  if (count === 0) return null;
  const columns = view === "grid" ? Math.max(1, layout.columns) : 1;
  const page = Math.max(1, layout.pageRows) * columns;
  const last = count - 1;
  const clamp = (value: number) => Math.max(0, Math.min(last, value));
  if (key === "Home") return 0;
  if (key === "End") return last;
  if ((key === "ArrowLeft" || key === "ArrowRight") && view !== "grid") return null;
  if (index < 0) return key === "ArrowUp" || key === "ArrowLeft" || key === "PageUp" ? last : 0;
  switch (key) {
    case "ArrowUp":
      return index - columns < 0 ? index : index - columns;
    case "ArrowDown":
      return index + columns > last ? index : index + columns;
    case "ArrowLeft":
      return clamp(index - 1);
    case "ArrowRight":
      return clamp(index + 1);
    case "PageUp":
      return clamp(index - page);
    case "PageDown":
      return clamp(index + page);
  }
}

/** Applies a keyboard move to `target` with Explorer modifier rules. */
export function applyNav(
  selection: Selection,
  order: string[],
  target: string,
  { shift, primary }: { shift: boolean; primary: boolean },
): Selection {
  if (shift && primary) return addRange(selection, order, target);
  if (shift) return selectRange(selection, order, target);
  if (primary) return { ...selection, focus: target };
  return selectOnly(target);
}

/** Index half a page away from `index` (-1 when nothing has focus), or null when the list is empty. */
export function halfPageTarget(index: number, direction: 1 | -1, layout: NavLayout): number | null {
  if (layout.count === 0) return null;
  const last = layout.count - 1;
  if (index < 0) return direction === 1 ? 0 : last;
  const columns = layout.view === "grid" ? Math.max(1, layout.columns) : 1;
  const step = Math.max(1, Math.floor(Math.max(1, layout.pageRows) / 2)) * columns;
  return Math.max(0, Math.min(last, index + direction * step));
}

/** Selects every listed path that was not selected. Focus stays where it was. */
export function invert(selection: Selection, order: string[]): Selection {
  const selected = new Set(selection.paths);
  const paths = order.filter((path) => !selected.has(path));
  return { paths, anchor: paths[0] ?? "", focus: selection.focus };
}

/** Yazi visual mode: `base` is the selection when the mode started. */
export type VisualState = { mode: "add" | "remove"; anchor: string; base: string[] };

/** The range from the anchor to `focus`, added to or removed from the starting selection. */
export function visualRange(order: string[], visual: VisualState, focus: string): Selection {
  const covered = new Set(range(order, visual.anchor, focus));
  const base = new Set(visual.base);
  const paths = order.filter((path) =>
    visual.mode === "add" ? base.has(path) || covered.has(path) : base.has(path) && !covered.has(path),
  );
  return { paths, anchor: visual.anchor, focus };
}

/** Where focus goes after `removed` leave the list: the next remaining item, else the previous one. */
export function focusAfterRemoval(order: string[], removed: string[]): string {
  const gone = new Set(removed);
  const last = order.reduce((found, path, index) => (gone.has(path) ? index : found), -1);
  if (last < 0) return "";
  const after = order.slice(last + 1).find((path) => !gone.has(path));
  if (after) return after;
  return order.slice(0, last).reverse().find((path) => !gone.has(path)) ?? "";
}
