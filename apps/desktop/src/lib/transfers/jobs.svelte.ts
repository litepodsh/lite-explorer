import { invoke } from "@tauri-apps/api/core";
import {
  clearCompleted,
  isRunning,
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
    void invoke("cancel_transfer", { id });
  }
}
