import { describe, expect, test as it } from "bun:test";
import {
  EMPTY_SELECTION,
  addRange,
  applyNav,
  navTarget,
  orderedPaths,
  prune,
  selectAll,
  selectOnly,
  selectRange,
  toggle,
} from "./selection.js";

const order = ["a", "b", "c", "d", "e", "f"];

describe("click selection", () => {
  it("selects only the clicked path", () => {
    expect(selectOnly("c")).toEqual({ paths: ["c"], anchor: "c", focus: "c" });
  });

  it("toggles a path in and out and moves the anchor", () => {
    let selection = toggle(selectOnly("a"), "c");
    expect(selection).toEqual({ paths: ["a", "c"], anchor: "c", focus: "c" });
    selection = toggle(selection, "a");
    expect(selection).toEqual({ paths: ["c"], anchor: "a", focus: "a" });
  });

  it("selects a range from the anchor in both directions", () => {
    expect(selectRange(selectOnly("b"), order, "d").paths).toEqual(["b", "c", "d"]);
    expect(selectRange(selectOnly("e"), order, "c").paths).toEqual(["c", "d", "e"]);
  });

  it("replaces the selection with the range but keeps the anchor", () => {
    let selection = selectRange(selectOnly("c"), order, "f");
    selection = selectRange(selection, order, "a");
    expect(selection).toEqual({ paths: ["a", "b", "c"], anchor: "c", focus: "a" });
  });

  it("starts the range at the target when the anchor is gone", () => {
    expect(selectRange(selectOnly("zz"), order, "d").paths).toEqual(["d"]);
  });

  it("adds a range to the existing selection", () => {
    const selection = addRange(toggle(selectOnly("a"), "d"), order, "f");
    expect(selection.paths).toEqual(["a", "d", "e", "f"]);
  });

  it("selects everything", () => {
    expect(selectAll(order).paths).toEqual(order);
    expect(selectAll([])).toEqual(EMPTY_SELECTION);
  });
});

describe("prune", () => {
  it("drops paths that are no longer listed", () => {
    const selection = { paths: ["a", "x", "c"], anchor: "x", focus: "c" };
    expect(prune(selection, order)).toEqual({ paths: ["a", "c"], anchor: "a", focus: "c" });
  });

  it("returns the same object when nothing changed", () => {
    const selection = selectOnly("b");
    expect(prune(selection, order)).toBe(selection);
  });
});

it("orders selected paths like the list", () => {
  expect(orderedPaths({ paths: ["e", "a", "c"], anchor: "", focus: "" }, order)).toEqual(["a", "c", "e"]);
});

describe("navTarget", () => {
  const list = { count: 6, columns: 1, pageRows: 2, view: "list" as const };
  const grid = { count: 10, columns: 3, pageRows: 2, view: "grid" as const };

  it("moves one row in the list and stops at the ends", () => {
    expect(navTarget(2, "ArrowDown", list)).toBe(3);
    expect(navTarget(0, "ArrowUp", list)).toBe(0);
    expect(navTarget(5, "ArrowDown", list)).toBe(5);
  });

  it("starts at the first or last item without focus", () => {
    expect(navTarget(-1, "ArrowDown", list)).toBe(0);
    expect(navTarget(-1, "ArrowUp", list)).toBe(5);
  });

  it("ignores Left and Right in the list", () => {
    expect(navTarget(2, "ArrowLeft", list)).toBeNull();
    expect(navTarget(2, "ArrowRight", list)).toBeNull();
  });

  it("pages by whole rows and clamps", () => {
    expect(navTarget(1, "PageDown", list)).toBe(3);
    expect(navTarget(5, "PageDown", list)).toBe(5);
    expect(navTarget(1, "PageUp", list)).toBe(0);
    expect(navTarget(2, "Home", list)).toBe(0);
    expect(navTarget(2, "End", list)).toBe(5);
  });

  it("moves by item and by row in the grid", () => {
    expect(navTarget(4, "ArrowLeft", grid)).toBe(3);
    expect(navTarget(4, "ArrowRight", grid)).toBe(5);
    expect(navTarget(4, "ArrowUp", grid)).toBe(1);
    expect(navTarget(4, "ArrowDown", grid)).toBe(7);
    expect(navTarget(8, "ArrowDown", grid)).toBe(8);
    expect(navTarget(1, "ArrowUp", grid)).toBe(1);
    expect(navTarget(0, "PageDown", grid)).toBe(6);
  });
});

describe("applyNav", () => {
  const current = selectOnly("b");

  it("selects only the target without modifiers", () => {
    expect(applyNav(current, order, "c", { shift: false, primary: false })).toEqual(selectOnly("c"));
  });

  it("extends from the anchor with Shift", () => {
    expect(applyNav(current, order, "d", { shift: true, primary: false }).paths).toEqual(["b", "c", "d"]);
  });

  it("moves only the focus with the primary modifier", () => {
    expect(applyNav(current, order, "d", { shift: false, primary: true })).toEqual({ paths: ["b"], anchor: "b", focus: "d" });
  });

  it("adds a range with both modifiers", () => {
    const selection = toggle(selectOnly("a"), "e");
    expect(applyNav(selection, order, "f", { shift: true, primary: true }).paths).toEqual(["a", "e", "f"]);
  });
});
