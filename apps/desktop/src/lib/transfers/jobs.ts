import type { FileProgress } from "./download-progress.js";

export type JobKind = "upload" | "download" | "copy" | "move" | "delete" | "send" | "receive" | "extract" | "create" | "rename" | "compress";
export type JobState = "active" | "paused" | "done" | "failed" | "cancelled";

export type Job = {
  id: string;
  kind: JobKind;
  label: string;
  destination: string;
  filesTotal: number;
  filesDone: number;
  bytesTotal: number;
  bytesDone: number;
  state: JobState;
  cancellable?: boolean;
  error?: string;
  startedAt: number;
  finishedAt?: number;
};

/** Payload of the `transfer-progress` event emitted by Rust. */
export type TransferEventPayload = {
  fileProgress?: FileProgress | null;
  id: string;
  kind: JobKind;
  label: string;
  destination: string;
  filesTotal: number;
  filesDone: number;
  bytesTotal: number;
  bytesDone: number;
  state: JobState;
  cancellable?: boolean;
  error?: string | null;
  startedAt?: number;
  finishedAt?: number | null;
};

export type JobFilter = "all" | "active" | "done" | "failed";

export function isRunning(job: Job): boolean {
  return job.state === "active" || job.state === "paused";
}

export function createJobsState(): Job[] {
  return [];
}

/** Inserts a new job or updates the matching one in place. `startedAt` is preserved once set. */
export function upsert(state: Job[], event: TransferEventPayload): Job[] {
  const existing = state.find((job) => job.id === event.id);
  const now = Date.now();
  const running = event.state === "active" || event.state === "paused";
  const next: Job = {
    id: event.id,
    kind: event.kind,
    label: event.label,
    destination: event.destination,
    filesTotal: event.filesTotal,
    filesDone: event.filesDone,
    bytesTotal: event.bytesTotal,
    bytesDone: event.bytesDone,
    state: event.state,
    error: event.error ?? undefined,
    cancellable: event.cancellable,
    startedAt: existing?.startedAt ?? event.startedAt ?? now,
    finishedAt: running ? undefined : existing?.finishedAt ?? event.finishedAt ?? now,
  };
  if (existing) return state.map((job) => (job.id === event.id ? next : job));
  return [...state, next];
}

export function visible(state: Job[], filter: JobFilter): Job[] {
  if (filter === "all") return state;
  if (filter === "active") return state.filter(isRunning);
  if (filter === "done") return state.filter((job) => job.state === "done");
  return state.filter((job) => job.state === "failed" || job.state === "cancelled");
}

export function clearCompleted(state: Job[]): Job[] {
  return state.filter((job) => isRunning(job) || job.state === "failed");
}

export function removeJob(state: Job[], id: string): Job[] {
  return state.filter((job) => job.id !== id);
}

/** Records operations without byte progress; their backend calls cannot be cancelled. */
export async function trackJob<T>(
  publish: (event: TransferEventPayload) => void,
  kind: JobKind,
  label: string,
  destination: string,
  operation: () => Promise<T>,
): Promise<T> {
  const event: TransferEventPayload = {
    id: crypto.randomUUID(), kind, label, destination,
    filesTotal: 0, filesDone: 0, bytesTotal: 0, bytesDone: 0,
    state: "active", cancellable: false, startedAt: Date.now(),
  };
  publish({ ...event });
  try {
    const result = await operation();
    publish({ ...event, state: "done", finishedAt: Date.now() });
    return result;
  } catch (error) {
    publish({ ...event, state: "failed", finishedAt: Date.now(), error: error instanceof Error ? error.message : String(error) });
    throw error;
  }
}

/** The mounted Activity panel receives file-operation updates through this callback. */
export const activity = {
  publish: (_event: TransferEventPayload) => {},
  track<T>(kind: JobKind, label: string, destination: string, operation: () => Promise<T>) {
    return trackJob((event) => this.publish(event), kind, label, destination, operation);
  },
};

/** Seconds retain millisecond precision, including for very short operations. */
export function formatJobDuration(job: Job, now: number): string {
  const milliseconds = Math.max(0, (job.finishedAt ?? now) - job.startedAt);
  const hours = Math.floor(milliseconds / 3_600_000);
  const minutes = Math.floor(milliseconds / 60_000) % 60;
  const seconds = ((milliseconds % 60_000) / 1000).toFixed(3);
  return `${hours ? `${hours}h ` : ""}${hours || minutes ? `${minutes}m ` : ""}${seconds}s`;
}
