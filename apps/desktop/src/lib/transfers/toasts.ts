import type { Job } from "./jobs.js";

export const TOAST_LIMIT = 4;
export const TOAST_DISMISS_MS = 3000;

/** Newest-first jobs that are not dismissed, capped at `limit`. */
export function visibleToasts(
  jobs: Job[],
  dismissed: Record<string, true>,
  limit = TOAST_LIMIT,
): Job[] {
  return jobs
    .filter((job) => !dismissed[job.id])
    .slice(-limit)
    .reverse();
}

export function overflowCount(
  jobs: Job[],
  dismissed: Record<string, true>,
  limit = TOAST_LIMIT,
): number {
  const count = jobs.filter((job) => !dismissed[job.id]).length;
  return Math.max(0, count - limit);
}

export function shouldAutoDismiss(job: Job, now: number): boolean {
  return (
    (job.state === "done" || job.state === "cancelled") &&
    job.finishedAt !== undefined &&
    now - job.finishedAt > TOAST_DISMISS_MS
  );
}
