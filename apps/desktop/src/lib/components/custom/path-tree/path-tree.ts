export type PathTreeDragHandlers = {
  start(paths: string[], event: PointerEvent): void;
  move(event: PointerEvent): void;
  drop(paths: string[], event: PointerEvent): void;
  cancel(): void;
};

type DatasetTarget = { dataset?: Record<string, string | undefined> } | null | undefined;

/** Path of the tree row an event started in. The tree renders in an open shadow root. */
export function rowPathFromEvent(path: readonly unknown[]): string | null {
  for (const target of path) {
    const dataset = (target as DatasetTarget)?.dataset;
    if (dataset?.type === "item" && dataset.itemPath) return dataset.itemPath;
  }
  return null;
}

/** A drag carries the selection when it starts on a selected row, otherwise only that row. */
export function pathsToDrag(pressed: string, selected: readonly string[]): string[] {
  return selected.includes(pressed) ? [...selected] : [pressed];
}

export function sameSelection(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  const set = new Set(a);
  return b.every((path) => set.has(path));
}
