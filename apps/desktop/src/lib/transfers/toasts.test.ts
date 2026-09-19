import { describe, expect, test } from "bun:test";
import type { Job } from "./jobs.js";
import { overflowCount, shouldAutoDismiss, visibleToasts } from "./toasts.js";

function job(overrides: Partial<Job>): Job {
  return {
    id: "job",
    kind: "delete",
    label: "Delete: a",
    destination: "",
    filesTotal: 1,
    filesDone: 1,
    bytesTotal: 0,
    bytesDone: 0,
    state: "done",
    startedAt: 0,
    ...overrides,
  };
}

describe("visibleToasts", () => {
  test("drops dismissed jobs and caps the list", () => {
    const jobs = [job({ id: "a" }), job({ id: "b" }), job({ id: "c" })];
    expect(visibleToasts(jobs, { b: true }, 2).map((item) => item.id)).toEqual(["c", "a"]);
  });
});

describe("overflowCount", () => {
  test("counts jobs beyond the limit", () => {
    const jobs = [job({ id: "a" }), job({ id: "b" }), job({ id: "c" })];
    expect(overflowCount(jobs, {}, 2)).toBe(1);
  });
});

describe("shouldAutoDismiss", () => {
  test("dismisses finished jobs after the delay only", () => {
    expect(shouldAutoDismiss(job({ state: "done", finishedAt: 0 }), 7000)).toBe(true);
    expect(shouldAutoDismiss(job({ state: "done", finishedAt: 0 }), 1000)).toBe(false);
    expect(shouldAutoDismiss(job({ state: "failed", finishedAt: 0 }), 7000)).toBe(false);
    expect(shouldAutoDismiss(job({ state: "active" }), 7000)).toBe(false);
  });
});
