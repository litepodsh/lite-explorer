import type { TransferEventPayload } from "./jobs.js";
import type { FileProgress } from "./download-progress.js";

/** Exact destination paths, including renamed collisions and nested downloads. */
export const fileDownloads = $state<{ jobs: Record<string, FileProgress> }>({ jobs: {} });

export function applyDownloadProgress(event: TransferEventPayload) {
  if (event.state === "active" && event.fileProgress) {
    fileDownloads.jobs[event.id] = event.fileProgress;
  } else {
    delete fileDownloads.jobs[event.id];
  }
}
