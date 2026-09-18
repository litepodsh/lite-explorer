import { describe, expect, test } from "bun:test";
import { nextRegion, type RegionSlot } from "./focus-cycle.js";

const slots: RegionSlot[] = [
  { region: "sidebar", paneId: null },
  { region: "list", paneId: "a" },
  { region: "preview", paneId: "a" },
  { region: "list", paneId: "b" },
];

describe("nextRegion", () => {
  test("moves forward and wraps", () => {
    expect(nextRegion(slots, { region: "list", paneId: "a" }, 1)).toEqual({
      region: "preview",
      paneId: "a",
    });
    expect(nextRegion(slots, { region: "list", paneId: "b" }, 1)).toEqual({
      region: "sidebar",
      paneId: null,
    });
  });

  test("moves backward and wraps", () => {
    expect(nextRegion(slots, { region: "sidebar", paneId: null }, -1)).toEqual({
      region: "list",
      paneId: "b",
    });
  });

  test("starts at an end when the current region is not listed", () => {
    expect(nextRegion(slots, { region: "preview", paneId: "b" }, 1)).toEqual({
      region: "sidebar",
      paneId: null,
    });
    expect(nextRegion(slots, null, -1)).toEqual({ region: "list", paneId: "b" });
  });

  test("returns null without regions", () => {
    expect(nextRegion([], null, 1)).toBeNull();
  });
});
