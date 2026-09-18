import { invoke } from "@tauri-apps/api/core";
import { message } from "@tauri-apps/plugin-dialog";
import {
  extractArchiveJob,
  planExtraction,
  type ExtractionTarget,
  type Resolution,
} from "$lib/file-ops/archive.js";
import type { TransferEventPayload } from "$lib/transfers/jobs.js";
import { runExtraction, type ExtractionRequest } from "./run-extraction.js";

export type PendingConflict = {
  target: ExtractionTarget;
  destination: string;
  resolve: (resolution: Resolution) => void;
};

export type ExtractionProgress = {
  jobId: string;
  filesDone: number;
  filesTotal: number;
  bytesDone: number;
  bytesTotal: number;
};

/** App-wide archive extraction: one flow for preview buttons, drops and the context menu. */
class Extraction {
  /** Conflicts waiting for a decision; the dialog shows the first. */
  conflicts = $state.raw<PendingConflict[]>([]);
  /** Running jobs by archive path, for inline progress in the preview. */
  running = $state.raw<Map<string, ExtractionProgress>>(new Map());
  /** Set by the page: refreshes panes and highlights what was extracted. */
  onExtracted: (paths: string[]) => void = () => {};

  async run(request: ExtractionRequest) {
    if (this.running.has(request.archive)) return;
    try {
      const result = await runExtraction(request, {
        plan: planExtraction,
        extract: extractArchiveJob,
        askConflict: (target, destination) => this.#ask(target, destination),
        newJobId: () => crypto.randomUUID(),
        onStart: (jobId) =>
          this.#setRunning(request.archive, {
            jobId,
            filesDone: 0,
            filesTotal: 0,
            bytesDone: 0,
            bytesTotal: 0,
          }),
      });
      if (result.status === "skipped") return;
      const { outcome } = result;
      if (outcome.extracted.length) this.onExtracted(outcome.extracted);
      if (outcome.failed.length) {
        const title =
          outcome.failed.length === 1
            ? `Couldn’t extract “${outcome.failed[0].name}”`
            : `Couldn’t extract ${outcome.failed.length} items`;
        await message(
          outcome.failed.map((failure) => `${failure.name}: ${failure.error}`).join("\n"),
          {
            title,
            kind: "error",
          },
        );
      }
    } catch (error) {
      await message(error instanceof Error ? error.message : String(error), {
        title: "Couldn’t extract",
        kind: "error",
      });
    } finally {
      this.#setRunning(request.archive, null);
    }
  }

  applyProgress(event: TransferEventPayload) {
    if (event.kind !== "extract") return;
    for (const [archive, progress] of this.running) {
      if (progress.jobId !== event.id) continue;
      this.#setRunning(archive, {
        jobId: event.id,
        filesDone: event.filesDone,
        filesTotal: event.filesTotal,
        bytesDone: event.bytesDone,
        bytesTotal: event.bytesTotal,
      });
    }
  }

  cancel(archive: string) {
    const progress = this.running.get(archive);
    if (progress) void invoke("cancel_transfer", { id: progress.jobId });
  }

  #ask(target: ExtractionTarget, destination: string): Promise<Resolution> {
    return new Promise((resolve) => {
      const conflict: PendingConflict = {
        target,
        destination,
        resolve: (resolution) => {
          this.conflicts = this.conflicts.filter((pending) => pending !== conflict);
          resolve(resolution);
        },
      };
      this.conflicts = [...this.conflicts, conflict];
    });
  }

  #setRunning(archive: string, progress: ExtractionProgress | null) {
    const next = new Map(this.running);
    if (progress) next.set(archive, progress);
    else next.delete(archive);
    this.running = next;
  }
}

export const extraction = new Extraction();
