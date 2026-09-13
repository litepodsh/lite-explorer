import { describe, expect, test } from "bun:test";
import { findMatches } from "./find-matches.js";

describe("findMatches", () => {
  test("is case-insensitive", () => {
    expect(findMatches("Lite explorer, LITE", "lite")).toEqual([
      [0, 4],
      [15, 19],
    ]);
  });

  test("returns non-overlapping matches", () => {
    expect(findMatches("aaaa", "aa")).toEqual([
      [0, 2],
      [2, 4],
    ]);
  });

  test("returns nothing for an empty query or no match", () => {
    expect(findMatches("anything", "")).toEqual([]);
    expect(findMatches("anything", "zzz")).toEqual([]);
  });

  test("stops at the limit", () => {
    expect(findMatches("a a a a", "a", 2)).toEqual([
      [0, 1],
      [2, 3],
    ]);
  });
});
