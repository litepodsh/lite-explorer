import { describe, expect, test } from "bun:test";
import {
  diskBreakdown,
  formatAge,
  isStale,
  STALE_AFTER_SECONDS,
  summarizeFolders,
  upsertEntry,
} from "./usage.js";
import type { FolderUsageEntry } from "./types.js";

const folder = (name: string, bytes: number): FolderUsageEntry => ({
  name,
  path: `/Users/me/${name}`,
  bytes,
  is_hidden: name.startsWith("."),
});

describe("isStale", () => {
  test("is stale before the first scan", () => {
    expect(isStale(null, 1000)).toBe(true);
  });

  test("turns stale after thirty minutes", () => {
    expect(isStale(1000, 1000 + STALE_AFTER_SECONDS - 1)).toBe(false);
    expect(isStale(1000, 1000 + STALE_AFTER_SECONDS)).toBe(true);
  });
});

describe("formatAge", () => {
  test("rounds down to the largest whole unit", () => {
    expect(formatAge(1000, 1030)).toBe("just now");
    expect(formatAge(1000, 1000 + 12 * 60 + 59)).toBe("12 min ago");
    expect(formatAge(1000, 1000 + 2 * 3600)).toBe("2 h ago");
    expect(formatAge(1000, 1000 + 3 * 86_400)).toBe("3 d ago");
  });
});

describe("diskBreakdown", () => {
  test("splits used space into home and everything else", () => {
    expect(diskBreakdown({ total_bytes: 1000, free_bytes: 200 }, 650)).toEqual({
      used: 800,
      home: 650,
      system: 150,
      free: 200,
    });
  });

  test("never lets home exceed used space or go negative", () => {
    expect(diskBreakdown({ total_bytes: 1000, free_bytes: 900 }, 500).home).toBe(100);
    expect(diskBreakdown({ total_bytes: 1000, free_bytes: 900 }, null)).toEqual({
      used: 100,
      home: 0,
      system: 100,
      free: 900,
    });
  });
});

describe("summarizeFolders", () => {
  const entries = [
    folder("Work", 500),
    folder(".local", 300),
    folder("Music", 50),
    folder("Code", 100),
  ];

  test("keeps the biggest folders and sums the rest", () => {
    const summary = summarizeFolders(entries, 1000, { limit: 2, showHidden: true });
    expect(summary.top.map((entry) => entry.name)).toEqual(["Work", ".local"]);
    expect(summary.restBytes).toBe(200);
    expect(summary.restHasHidden).toBe(false);
  });

  test("folds hidden folders into the rest when they are not shown", () => {
    const summary = summarizeFolders(entries, 1000, { limit: 2, showHidden: false });
    expect(summary.top.map((entry) => entry.name)).toEqual(["Work", "Code"]);
    expect(summary.restBytes).toBe(400);
    expect(summary.restHasHidden).toBe(true);
  });
});

describe("upsertEntry", () => {
  test("replaces the older size and keeps the list sorted", () => {
    const next = upsertEntry([folder("Work", 500), folder("Code", 100)], folder("Code", 900));
    expect(next).toEqual([folder("Code", 900), folder("Work", 500)]);
  });
});
