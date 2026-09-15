import { describe, expect, test } from "bun:test";
import { pathsToDrag, rowPathFromEvent, sameSelection } from "./path-tree.js";

describe("rowPathFromEvent", () => {
  test("returns the innermost tree row in the composed event path", () => {
    const path = [
      { dataset: {} },
      { dataset: { type: "item", itemPath: "src/lib/" } },
      { dataset: { type: "item", itemPath: "src/" } },
    ];
    expect(rowPathFromEvent(path)).toBe("src/lib/");
  });

  test("ignores events outside rows", () => {
    expect(rowPathFromEvent([{}, null, { dataset: { type: "header" } }])).toBeNull();
  });
});

describe("pathsToDrag", () => {
  test("drags the whole selection when the pressed row is selected", () => {
    expect(pathsToDrag("a", ["a", "b"])).toEqual(["a", "b"]);
  });

  test("drags only the pressed row otherwise", () => {
    expect(pathsToDrag("c", ["a", "b"])).toEqual(["c"]);
  });
});

describe("sameSelection", () => {
  test("compares selections regardless of order", () => {
    expect(sameSelection(["a", "b"], ["b", "a"])).toBe(true);
    expect(sameSelection(["a"], ["a", "b"])).toBe(false);
  });
});
