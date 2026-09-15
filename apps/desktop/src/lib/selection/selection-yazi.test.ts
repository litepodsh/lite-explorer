import { describe, expect, test } from "bun:test";
import { focusAfterRemoval, halfPageTarget, invert, visualRange, type NavLayout } from "./selection.js";

const order = ["a", "b", "c", "d", "e", "f"];
const list: NavLayout = { count: 6, columns: 1, pageRows: 4, view: "list" };

describe("halfPageTarget", () => {
  test("moves half the visible rows and clamps", () => {
    expect(halfPageTarget(0, 1, list)).toBe(2);
    expect(halfPageTarget(5, 1, list)).toBe(5);
    expect(halfPageTarget(1, -1, list)).toBe(0);
  });

  test("moves whole rows in the grid view", () => {
    expect(halfPageTarget(0, 1, { count: 20, columns: 3, pageRows: 4, view: "grid" })).toBe(6);
  });

  test("starts at an end without focus and does nothing when empty", () => {
    expect(halfPageTarget(-1, 1, list)).toBe(0);
    expect(halfPageTarget(-1, -1, list)).toBe(5);
    expect(halfPageTarget(0, 1, { ...list, count: 0 })).toBeNull();
  });
});

describe("invert", () => {
  test("selects what was not selected and keeps focus", () => {
    expect(invert({ paths: ["b", "d"], anchor: "b", focus: "d" }, order)).toEqual({
      paths: ["a", "c", "e", "f"],
      anchor: "a",
      focus: "d",
    });
  });
});

describe("visualRange", () => {
  test("add mode joins the range to the starting selection", () => {
    const visual = { mode: "add" as const, anchor: "c", base: ["a"] };
    expect(visualRange(order, visual, "e")).toEqual({ paths: ["a", "c", "d", "e"], anchor: "c", focus: "e" });
    expect(visualRange(order, visual, "b")).toEqual({ paths: ["a", "b", "c"], anchor: "c", focus: "b" });
  });

  test("remove mode takes the range out of the starting selection", () => {
    const visual = { mode: "remove" as const, anchor: "b", base: order };
    expect(visualRange(order, visual, "d").paths).toEqual(["a", "e", "f"]);
  });
});

describe("focusAfterRemoval", () => {
  test("prefers the next remaining item", () => {
    expect(focusAfterRemoval(order, ["b", "c"])).toBe("d");
  });

  test("falls back to the previous item at the end", () => {
    expect(focusAfterRemoval(order, ["e", "f"])).toBe("d");
  });

  test("is empty when nothing remains or nothing was removed", () => {
    expect(focusAfterRemoval(["a"], ["a"])).toBe("");
    expect(focusAfterRemoval(order, [])).toBe("");
  });
});
