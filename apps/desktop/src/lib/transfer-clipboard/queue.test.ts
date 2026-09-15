import { describe, expect, test } from "bun:test";
import { appendEntry, applyTransferEvent, emptyQueue, etaSeconds, formatEta } from "./queue.js";

describe("transfer clipboard queue", () => {
  test("replaces a queued source instead of duplicating it", () => {
    let queue = appendEntry(
      emptyQueue(),
      { path: "/a/report.pdf", name: "report.pdf", isDirectory: false, mode: "copy" },
      "one",
    );
    queue = appendEntry(
      queue,
      { path: "/a/report.pdf", name: "report.pdf", isDirectory: false, mode: "move" },
      "two",
    );
    expect(queue.items).toEqual([
      {
        id: "one",
        path: "/a/report.pdf",
        name: "report.pdf",
        isDirectory: false,
        mode: "move",
        state: "queued",
      },
    ]);
  });

  test("calculates ETA only after measurable byte progress", () => {
    let queue = appendEntry(
      emptyQueue(),
      { path: "/a/report.pdf", name: "report.pdf", isDirectory: false, mode: "copy" },
      "one",
    );
    queue = applyTransferEvent(
      queue,
      { jobId: "job", itemId: "one", state: "transferring", bytesDone: 0, bytesTotal: 100 },
      1_000,
    );
    expect(etaSeconds(queue, 1_000)).toBeNull();
    queue = applyTransferEvent(
      queue,
      { jobId: "job", itemId: "one", state: "transferring", bytesDone: 50, bytesTotal: 100 },
      2_000,
    );
    expect(etaSeconds(queue, 2_000)).toBe(1);
    expect(formatEta(etaSeconds(queue, 2_000))).toBe("1s left");
  });
});
