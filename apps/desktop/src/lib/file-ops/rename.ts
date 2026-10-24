import { invoke } from "@tauri-apps/api/core";
import type { RenamePlan, RenameStep } from "./rename-steps.js";
import { toWire } from "./rename-steps.js";

export type RenameOutcome = { renamed: { from: string; to: string }[] };

/** The preview: a new name and a status for each path, in the order given. */
export function renamePlan(paths: string[], steps: readonly RenameStep[]): Promise<RenamePlan> {
  return invoke<RenamePlan>("rename_plan", { paths, steps: toWire(steps) });
}

/** Renames everything or nothing. Progress goes through the shared `transfer-progress` events. */
export function renameBatch(paths: string[], steps: readonly RenameStep[]): Promise<RenameOutcome> {
  return invoke<RenameOutcome>("rename_batch", {
    jobId: crypto.randomUUID(),
    paths,
    steps: toWire(steps),
  });
}
