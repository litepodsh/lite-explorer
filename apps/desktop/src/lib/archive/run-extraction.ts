import type { ExtractionJobRequest, ExtractionOutcome, ExtractionTarget, Resolution } from "$lib/file-ops/archive.js";

export type ExtractionRequest = { archive: string; destination: string; entries?: string[] };

export type ExtractionDeps = {
  plan: (archive: string, destination: string, entries?: string[]) => Promise<ExtractionTarget[]>;
  /** Resolves with the user's choice for one existing target. */
  askConflict: (target: ExtractionTarget, destination: string) => Promise<Resolution>;
  extract: (request: ExtractionJobRequest) => Promise<ExtractionOutcome>;
  newJobId: () => string;
  onStart?: (jobId: string) => void;
};

export type ExtractionResult = { status: "skipped" } | { status: "finished"; jobId: string; outcome: ExtractionOutcome };

/** Plans the extraction, asks about each existing target in order, then extracts. */
export async function runExtraction(request: ExtractionRequest, deps: ExtractionDeps): Promise<ExtractionResult> {
  const targets = await deps.plan(request.archive, request.destination, request.entries);
  const resolutions: Record<string, Resolution> = {};
  for (const target of targets) {
    if (target.exists) resolutions[target.name] = await deps.askConflict(target, request.destination);
  }
  if (targets.every((target) => resolutions[target.name] === "skip")) return { status: "skipped" };
  const jobId = deps.newJobId();
  deps.onStart?.(jobId);
  const outcome = await deps.extract({ jobId, ...request, resolutions });
  return { status: "finished", jobId, outcome };
}
