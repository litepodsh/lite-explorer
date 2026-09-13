import { describe, expect, test } from "bun:test";
import {
  clearCompleted,
  createJobsState,
  isRunning,
  removeJob,
  upsert,
  visible,
  type TransferEventPayload,
} from "./jobs.js";

function event(patch: Partial<TransferEventPayload> = {}): TransferEventPayload {
  return {
    id: "j1",
    kind: "upload",
    label: "a.txt",
    destination: "/dest",
    filesTotal: 3,
    filesDone: 0,
    bytesTotal: 300,
    bytesDone: 0,
    state: "active",
    error: null,
    ...patch,
  };
}

describe("upsert", () => {
  test("creates a job on first event", () => {
    const state = upsert(createJobsState(), event());
    expect(state.length).toBe(1);
    expect(state[0].id).toBe("j1");
    expect(state[0].state).toBe("active");
  });

  test("updates the existing job in place and preserves startedAt", () => {
    const first = upsert(createJobsState(), event());
    const startedAt = first[0].startedAt;
    const second = upsert(first, event({ filesDone: 2, bytesDone: 200 }));
    expect(second.length).toBe(1);
    expect(second[0].filesDone).toBe(2);
    expect(second[0].bytesDone).toBe(200);
    expect(second[0].startedAt).toBe(startedAt);
  });

  test("carries the terminal state and error", () => {
    const failed = upsert(createJobsState(), event({ state: "failed", error: "boom" }));
    expect(failed[0].state).toBe("failed");
    expect(failed[0].error).toBe("boom");
  });
});

describe("visible", () => {
  const state = [
    ...upsert(createJobsState(), event({ id: "a", state: "active" })),
    ...upsert(createJobsState(), event({ id: "d", state: "done" })),
    ...upsert(createJobsState(), event({ id: "f", state: "failed" })),
    ...upsert(createJobsState(), event({ id: "c", state: "cancelled" })),
  ];

  test("all returns everything", () => {
    expect(visible(state, "all").length).toBe(4);
  });
  test("active returns running jobs", () => {
    expect(visible(state, "active").map((job) => job.id)).toEqual(["a"]);
  });
  test("done and failed filter by state", () => {
    expect(visible(state, "done").map((job) => job.id)).toEqual(["d"]);
    expect(visible(state, "failed").map((job) => job.id)).toEqual(["f", "c"]);
  });
});

describe("clearCompleted", () => {
  test("removes done jobs but keeps running and failed", () => {
    const state = [
      ...upsert(createJobsState(), event({ id: "a", state: "active" })),
      ...upsert(createJobsState(), event({ id: "d", state: "done" })),
      ...upsert(createJobsState(), event({ id: "f", state: "failed" })),
    ];
    expect(clearCompleted(state).map((job) => job.id)).toEqual(["a", "f"]);
  });
});

describe("removeJob / isRunning", () => {
  test("removeJob drops one job", () => {
    const state = upsert(createJobsState(), event());
    expect(removeJob(state, "j1").length).toBe(0);
  });
  test("isRunning covers active and paused", () => {
    const base = {
      id: "j",
      kind: "upload" as const,
      label: "a",
      destination: "/d",
      filesTotal: 1,
      filesDone: 0,
      bytesTotal: 1,
      bytesDone: 0,
      startedAt: 0,
    };
    expect(isRunning({ ...base, state: "active" })).toBe(true);
    expect(isRunning({ ...base, state: "paused" })).toBe(true);
    expect(isRunning({ ...base, state: "done" })).toBe(false);
  });
});
