export type ClipboardMode = "copy" | "move";
export type ClipboardItemState = "queued" | "transferring" | "conflict" | "done" | "failed" | "skipped";

export type QueueEntry = {
  path: string;
  name: string;
  isDirectory: boolean;
  mode: ClipboardMode;
};

export type ClipboardItem = QueueEntry & {
  id: string;
  state: ClipboardItemState;
  error?: string;
  bytesDone?: number;
  bytesTotal?: number;
};

export type ClipboardTransferEvent = {
  jobId: string;
  itemId: string;
  state: ClipboardItemState;
  bytesDone?: number;
  bytesTotal?: number;
  error?: string | null;
};

export type ClipboardBatchRequest = {
  destination: string;
  entries: Pick<ClipboardItem, "id" | "path" | "mode">[];
};

export type QueueState = {
  items: ClipboardItem[];
  startedAt: number | null;
  bytesAtStart: number;
};

export const emptyQueue = (): QueueState => ({ items: [], startedAt: null, bytesAtStart: 0 });

export function appendEntry(state: QueueState, entry: QueueEntry, id: string): QueueState {
  const existing = state.items.find((item) => item.path === entry.path);
  if (existing) {
    return {
      ...state,
      items: state.items.map((item) =>
        item.path === entry.path ? { ...item, ...entry, state: "queued", error: undefined } : item,
      ),
    };
  }
  return { ...state, items: [...state.items, { ...entry, id, state: "queued" }] };
}

export function removeItem(state: QueueState, id: string): QueueState {
  return { ...state, items: state.items.filter((item) => item.id !== id) };
}

export function clearFinished(state: QueueState): QueueState {
  return { ...state, items: state.items.filter((item) => item.state !== "done" && item.state !== "skipped") };
}

export function batchRequest(state: QueueState, destination: string): ClipboardBatchRequest | null {
  const entries = state.items
    .filter((item) => item.state === "queued" || item.state === "failed")
    .map(({ id, path, mode }) => ({ id, path, mode }));
  return entries.length ? { destination, entries } : null;
}

export function applyTransferEvent(state: QueueState, event: ClipboardTransferEvent, now: number): QueueState {
  const previousBytes = totalBytes(state.items);
  const items = state.items.map((item) =>
    item.id === event.itemId
      ? {
          ...item,
          state: event.state,
          bytesDone: event.bytesDone ?? item.bytesDone,
          bytesTotal: event.bytesTotal ?? item.bytesTotal,
          error: event.error ?? undefined,
        }
      : item,
  );
  const startedAt = state.startedAt ?? (event.state === "transferring" ? now : null);
  return { ...state, items, startedAt, bytesAtStart: state.startedAt === null ? previousBytes : state.bytesAtStart };
}

export function totalBytes(items: ClipboardItem[]): number {
  return items.reduce((sum, item) => sum + (item.bytesDone ?? 0), 0);
}

export function totalBytesKnown(items: ClipboardItem[]): number | null {
  return items.every((item) => item.bytesTotal !== undefined)
    ? items.reduce((sum, item) => sum + (item.bytesTotal ?? 0), 0)
    : null;
}

export function etaSeconds(state: QueueState, now: number): number | null {
  const total = totalBytesKnown(state.items);
  if (total === null || state.startedAt === null) return null;
  const elapsed = (now - state.startedAt) / 1000;
  const bytes = totalBytes(state.items) - state.bytesAtStart;
  if (elapsed <= 0 || bytes <= 0) return null;
  return Math.max(0, (total - totalBytes(state.items)) / (bytes / elapsed));
}

export function formatEta(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds)) return null;
  if (seconds < 60) return `${Math.ceil(seconds)}s left`;
  return `${Math.ceil(seconds / 60)}m left`;
}
