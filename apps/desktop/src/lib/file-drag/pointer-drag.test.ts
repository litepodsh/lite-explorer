import { describe, expect, test } from "bun:test";
import { DRAG_THRESHOLD, passedThreshold } from "./pointer-drag";

describe("passedThreshold", () => {
  test("small movements stay a click", () => {
    expect(passedThreshold({ x: 10, y: 10 }, { x: 12, y: 11 })).toBe(false);
  });

  test("moving the threshold distance in any direction starts a drag", () => {
    expect(passedThreshold({ x: 10, y: 10 }, { x: 10 + DRAG_THRESHOLD, y: 10 })).toBe(true);
    expect(passedThreshold({ x: 10, y: 10 }, { x: 7, y: 6 })).toBe(true);
  });
});
