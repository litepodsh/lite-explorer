import { describe, expect, test } from "bun:test";
import type { ExtractionJobRequest, ExtractionOutcome, ExtractionTarget, Resolution } from "$lib/file-ops/archive.js";
import { runExtraction, type ExtractionDeps } from "./run-extraction.js";

const target = (name: string, exists: boolean): ExtractionTarget => ({
  name,
  isDirectory: true,
  exists,
  existingModified: null,
});

const outcome: ExtractionOutcome = { extracted: ["/dest/project"], failed: [], cancelled: false };
const request = { archive: "/downloads/a.zip", destination: "/dest" };

function fakes(targets: ExtractionTarget[], answers: Record<string, Resolution> = {}) {
  const asked: string[] = [];
  const extracted: ExtractionJobRequest[] = [];
  const started: string[] = [];
  const deps: ExtractionDeps = {
    plan: async () => targets,
    askConflict: async (conflict) => {
      asked.push(conflict.name);
      return answers[conflict.name];
    },
    extract: async (job) => {
      extracted.push(job);
      return outcome;
    },
    newJobId: () => "job-1",
    onStart: (jobId) => started.push(jobId),
  };
  return { deps, asked, extracted, started };
}

describe("runExtraction", () => {
  test("extracts without asking when no target exists", async () => {
    const { deps, asked, extracted, started } = fakes([target("project", false)]);
    const result = await runExtraction(request, deps);
    expect(asked).toEqual([]);
    expect(started).toEqual(["job-1"]);
    expect(extracted).toEqual([{ jobId: "job-1", ...request, resolutions: {} }]);
    expect(result).toEqual({ status: "finished", jobId: "job-1", outcome });
  });

  test("asks once per existing target in order and passes the answers", async () => {
    const { deps, asked, extracted } = fakes([target("a", true), target("b", false), target("c", true)], {
      a: "keepBoth",
      c: "replace",
    });
    await runExtraction({ ...request, entries: ["x/a", "b", "c"] }, deps);
    expect(asked).toEqual(["a", "c"]);
    expect(extracted[0].resolutions).toEqual({ a: "keepBoth", c: "replace" });
    expect(extracted[0].entries).toEqual(["x/a", "b", "c"]);
  });

  test("does not start a job when every target is skipped", async () => {
    const { deps, extracted, started } = fakes([target("a", true)], { a: "skip" });
    const result = await runExtraction(request, deps);
    expect(result).toEqual({ status: "skipped" });
    expect(extracted).toEqual([]);
    expect(started).toEqual([]);
  });
});
