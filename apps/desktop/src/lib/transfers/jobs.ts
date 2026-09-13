export type JobKind = "upload" | "download" | "copy" | "move" | "delete" | "send" | "receive";
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
  error?: string;
  startedAt: number;
};

/** Payload of the `transfer-progress` event emitted by Rust. */
export type TransferEventPayload = {
  id: string;
  kind: JobKind;
  label: string;
  destination: string;
  filesTotal: number;
  filesDone: number;
  bytesTotal: number;
  bytesDone: number;
  state: JobState;
  error?: string | null;
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
    startedAt: existing?.startedAt ?? Date.now(),
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
