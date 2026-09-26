import { describe, expect, test } from "bun:test";
import {
  activity,
  trackJob,
  formatJobDuration,
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

test("file activities publish active and terminal states, preserving results and failures", async () => {
  const events: TransferEventPayload[] = [];
  const publish = (event: TransferEventPayload) => {
    events.push(event);
  };
  let finish!: (value: string) => void;
  const pending = trackJob(
    publish,
    "copy",
    "Copy: a.txt",
    "/dest",
    () =>
      new Promise<string>((resolve) => {
        finish = resolve;
      }),
  );
  expect(events.map((event) => event.state)).toEqual(["active"]);
  expect(events[0].cancellable).toBe(false);
  finish("copied path");
  expect(await pending).toBe("copied path");
  expect(events.map((event) => event.state)).toEqual(["active", "done"]);
  expect(events[1].id).toBe(events[0].id);
  const failure = new Error("Permission denied");
  const caught = await trackJob(publish, "delete", "Delete: b.txt", "", async () => {
    throw failure;
  }).catch((error) => error);
  expect(caught).toBe(failure);
  expect(events.slice(2).map((event) => event.state)).toEqual(["active", "failed"]);
  expect(events[3].error).toBe("Permission denied");
  expect(events[2].id === events[0].id).toBe(false);
  const state = events.reduce(upsert, createJobsState());
  expect(visible(state, "done").length).toBe(1);
  expect(visible(state, "failed").length).toBe(1);
});

test("generic actions publish active and terminal states", async () => {
  const published: TransferEventPayload[] = [];
  const original = activity.publish;
  activity.publish = (event) => published.push(event);
  expect(await activity.action("Eject: USB", "/Volumes/USB", async () => "ok")).toBe("ok");
  const error = new Error("busy");
  expect(
    await activity
      .action("Clear Recents", "", async () => {
        throw error;
      })
      .catch((reason) => reason),
  ).toBe(error);
  activity.publish = original;
  expect(published.map((event) => [event.kind, event.state])).toEqual([
    ["action", "active"],
    ["action", "done"],
    ["action", "active"],
    ["action", "failed"],
  ]);
  expect(published[3].error).toBe("busy");
});

test("activity timestamps cover every kind and freeze on every terminal state", () => {
  const kinds = [
    "copy",
    "move",
    "delete",
    "create",
    "rename",
    "compress",
    "extract",
    "upload",
    "download",
    "send",
    "receive",
    "action",
  ] as const;
  for (const kind of kinds) {
    const first = upsert([], event({ kind, startedAt: 1000 }));
    const paused = upsert(first, event({ kind, state: "paused", startedAt: 2000 }));
    expect(paused[0].startedAt).toBe(1000);
    expect(paused[0].finishedAt).toBeUndefined();
    expect(formatJobDuration(paused[0], 2250)).toBe("1.250s");
    for (const state of ["done", "failed", "cancelled"] as const) {
      const ended = upsert(paused, event({ kind, state, startedAt: 1000, finishedAt: 2123 }));
      const duplicate = upsert(ended, event({ kind, state, finishedAt: 9000 }));
      expect(duplicate[0].finishedAt).toBe(2123);
      expect(formatJobDuration(duplicate[0], 99000)).toBe("1.123s");
    }
  }
  const short = upsert([], event({ state: "done", startedAt: 1000, finishedAt: 1007 }))[0];
  expect(formatJobDuration(short, 99000)).toBe("0.007s");
  expect(formatJobDuration({ ...short, finishedAt: 3_662_007 }, 0)).toBe("1h 1m 1.007s");
  expect(formatJobDuration({ ...short, finishedAt: 999 }, 0)).toBe("0.000s");
});

describe("activity.start / fail", () => {
  test("start publishes an active cancellable job and returns its id", () => {
    const published: TransferEventPayload[] = [];
    const original = activity.publish;
    activity.publish = (event) => published.push(event);
    const id = activity.start("delete", "Delete: 3 items", "");
    activity.publish = original;
    expect(published.length).toBe(1);
    expect(published[0].id).toBe(id);
    expect(published[0].kind).toBe("delete");
    expect(published[0].state).toBe("active");
    expect(published[0].cancellable).toBe(true);
    expect(published[0].filesTotal).toBe(0);
    expect(published[0].filesDone).toBe(0);
  });

  test("fail marks the job failed with a string error", () => {
    const published: TransferEventPayload[] = [];
    const original = activity.publish;
    activity.publish = (event) => published.push(event);
    activity.fail("job-1", "copy", "Copy: a", "/tmp", new Error("nope"));
    activity.publish = original;
    expect(published[0].id).toBe("job-1");
    expect(published[0].state).toBe("failed");
    expect(published[0].error).toBe("nope");
  });
});
