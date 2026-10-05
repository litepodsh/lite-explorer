import { invoke } from "@tauri-apps/api/core";
import {
  clearCompleted,
  isRunning,
  jobErrorMessage,
  removeJob,
  upsert,
  visible,
  type Job,
  type JobFilter,
  type TransferEventPayload,
} from "./jobs.js";

export class JobsStore {
  #jobs = $state<Job[]>([]);

  jobs = $derived(this.#jobs);
  activeCount = $derived(this.#jobs.filter(isRunning).length);

  upsert(event: TransferEventPayload) {
    this.#jobs = upsert(this.#jobs, event);
  }

  visible(filter: JobFilter): Job[] {
    return visible(this.#jobs, filter);
  }

  clearCompleted() {
    this.#jobs = clearCompleted(this.#jobs);
  }

  remove(id: string) {
    this.#jobs = removeJob(this.#jobs, id);
  }

  cancel(id: string) {
    void invoke("cancel_transfer", { id }).catch(() => {});
  }

  /** Re-runs a failed job with its original arguments (or its failed paths). */
  retry(id: string) {
    const job = this.#jobs.find((candidate) => candidate.id === id);
    const target = job?.failedPaths?.length && job.retryFailed ? job.retryFailed : job?.retry;
    if (!target) return;
    const base = {
      id,
      kind: job!.kind,
      label: job!.label,
      destination: job!.destination,
      filesTotal: 0,
      filesDone: 0,
      bytesTotal: 0,
      bytesDone: 0,
    };
    this.upsert({ ...base, state: "active", cancellable: true, startedAt: Date.now() });
    void target().then(
      () => this.upsert({ ...base, state: "done", finishedAt: Date.now() }),
      (error: unknown) =>
        this.upsert({
          ...base,
          state: "failed",
          error: jobErrorMessage(error),
          finishedAt: Date.now(),
        }),
    );
  }
}
