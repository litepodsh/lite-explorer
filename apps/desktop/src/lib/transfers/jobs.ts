import type { FileProgress } from "./download-progress.js";

/** AppError returned by the Rust commands (Fase 1): `{ kind, path?, message?, failed? }`. */
export type AppErrorLike = {
  kind?: string;
  path?: string;
  message?: string;
  completed?: number;
  total?: number;
  failed?: { path: string; error: string }[];
};

/** Turns any thrown value (string, Error, or serialized AppError) into drawer copy. */
export function jobErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  if (typeof error === "object" && error !== null) {
    const candidate = error as AppErrorLike;
    if (candidate.kind === "partial") {
      const completed = candidate.completed ?? 0;
      const total = candidate.total ?? completed + (candidate.failed?.length ?? 0);
      const first = candidate.failed?.[0]?.path;
      const detail = first ? `, starting with ${first}` : "";
      return `Completed ${completed} of ${total} items. ${candidate.failed?.length ?? 0} failed${detail}.`;
    }
    if (typeof candidate.message === "string" && candidate.message) return candidate.message;
    if (typeof candidate.kind === "string") {
      const path = candidate.path ? ` on ${candidate.path}` : "";
      return `Operation failed${path} (${candidate.kind}).`;
    }
  }
  return String(error);
}

export type JobKind =
  | "upload"
  | "download"
  | "copy"
  | "move"
  | "delete"
  | "send"
  | "receive"
  | "extract"
  | "create"
  | "rename"
  | "compress"
  | "action";
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
  /** Re-runs the job with its original arguments. Set locally (not by Rust). */
  retry?: () => Promise<unknown>;
  /** Re-runs only the paths that failed in a `Partial` batch error. */
  retryFailed?: () => Promise<unknown>;
  /** Paths from the `Partial` error; drives the "Retry N items" label. */
  failedPaths?: string[];
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
  /**
   * Local-only, set by `trackJob`/`activity.fail` so the drawer can retry a
   * failed job with its original arguments. Never sent by the Rust backend.
   */
  retry?: () => Promise<unknown>;
  retryFailed?: () => Promise<unknown>;
  failedPaths?: string[];
  startedAt?: number;
  finishedAt?: number | null;
  /** Set once when a whole queued item finished, so lists can drop it early. */
  item?: string | null;
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
    // Retry handlers live on the local Job; keep the latest non-undefined value
    // so an active re-run event does not wipe a failed job's retry closure.
    retry: event.retry ?? existing?.retry,
    retryFailed: event.retryFailed ?? existing?.retryFailed,
    failedPaths: event.failedPaths ?? existing?.failedPaths,
    startedAt: existing?.startedAt ?? event.startedAt ?? now,
    finishedAt: running ? undefined : (existing?.finishedAt ?? event.finishedAt ?? now),
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
    id: crypto.randomUUID(),
    kind,
    label,
    destination,
    filesTotal: 0,
    filesDone: 0,
    bytesTotal: 0,
    bytesDone: 0,
    state: "active",
    cancellable: false,
    startedAt: Date.now(),
  };
  publish({ ...event });
  try {
    const result = await operation();
    publish({ ...event, state: "done", finishedAt: Date.now() });
    return result;
  } catch (error) {
    publish({
      ...event,
      state: "failed",
      finishedAt: Date.now(),
      error: jobErrorMessage(error),
      retry: () => operation(),
    });
    throw error;
  }
}

/** The mounted Activity panel receives file-operation updates through this callback. */
export const activity = {
  publish: (_event: TransferEventPayload) => {},
  start(kind: JobKind, label: string, destination: string): string {
    const id = crypto.randomUUID();
    this.publish({
      id,
      kind,
      label,
      destination,
      filesTotal: 0,
      filesDone: 0,
      bytesTotal: 0,
      bytesDone: 0,
      state: "active",
      cancellable: true,
      startedAt: Date.now(),
    });
    return id;
  },
  fail(
    id: string,
    kind: JobKind,
    label: string,
    destination: string,
    error: unknown,
    retry?: {
      retry?: () => Promise<unknown>;
      retryFailed?: () => Promise<unknown>;
      failedPaths?: string[];
    },
  ): void {
    this.publish({
      id,
      kind,
      label,
      destination,
      filesTotal: 0,
      filesDone: 0,
      bytesTotal: 0,
      bytesDone: 0,
      state: "failed",
      error: jobErrorMessage(error),
      retry: retry?.retry,
      retryFailed: retry?.retryFailed,
      failedPaths: retry?.failedPaths,
      startedAt: Date.now(),
      finishedAt: Date.now(),
    });
  },
  track<T>(kind: JobKind, label: string, destination: string, operation: () => Promise<T>) {
    return trackJob((event) => this.publish(event), kind, label, destination, operation);
  },
  action<T>(label: string, destination: string, operation: () => Promise<T>) {
    return trackJob((event) => this.publish(event), "action", label, destination, operation);
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
