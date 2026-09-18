import { describe, expect, test } from "bun:test";
import {
  SWIPE_COMMIT,
  directionFor,
  isAvailable,
  shouldCommit,
  swipeTransforms,
} from "./gesture.js";

describe("direction", () => {
  test("positive is back, negative is forward", () => {
    expect(directionFor(1)).toBe("back");
    expect(directionFor(0.1)).toBe("back");
    expect(directionFor(-0.1)).toBe("forward");
  });

  test("availability follows history", () => {
    expect(isAvailable("back", true, false)).toBe(true);
    expect(isAvailable("back", false, true)).toBe(false);
    expect(isAvailable("forward", false, true)).toBe(true);
  });
});

describe("commit", () => {
  test("past the threshold in an available direction commits", () => {
    expect(shouldCommit(SWIPE_COMMIT + 0.01, true, false)).toBe(true);
    expect(shouldCommit(-(SWIPE_COMMIT + 0.01), false, true)).toBe(true);
    expect(shouldCommit(1, true, true)).toBe(true);
  });

  test("below the threshold or without history does not commit", () => {
    expect(shouldCommit(SWIPE_COMMIT - 0.01, true, true)).toBe(false);
    expect(shouldCommit(1, false, false)).toBe(false);
    expect(shouldCommit(1, false, true)).toBe(false);
    expect(shouldCommit(-1, true, false)).toBe(false);
  });
});

describe("layer transforms", () => {
  test("at rest both layers sit at zero", () => {
    expect(swipeTransforms(0, true, true)).toEqual({ current: 0, incoming: 0 });
  });

  test("a back drag moves the current layer right and reveals the previous folder from the left", () => {
    const { current, incoming } = swipeTransforms(0.5, true, false);
    expect(current).toBe(50);
    expect(incoming).toBe(-50);
  });

  test("a forward drag moves the current layer left and reveals the next folder from the right", () => {
    const { current, incoming } = swipeTransforms(-0.5, false, true);
    expect(current).toBe(-50);
    expect(incoming).toBe(50);
  });

  test("the current layer never slides past a full width", () => {
    expect(swipeTransforms(2, true, true).current).toBe(100);
    expect(swipeTransforms(-2, true, true).current).toBe(-100);
  });

  test("a direction without history is resisted", () => {
    const { current, incoming } = swipeTransforms(0.5, false, true);
    expect(current > 0).toBe(true);
    expect(current < 50).toBe(true);
    expect(incoming).toBe(current - 100);
  });
});
