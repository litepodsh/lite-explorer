import { expect, test } from "bun:test";
import { isWithinFolder, shouldCalculateSizes } from "./automatic-sizes.js";

test("matches home and nested folders without matching sibling prefixes", () => {
  const settings = { automaticSizesInHome: true, automaticSizePaths: [] };
  expect(shouldCalculateSizes("/home/me", "/home/me/", settings)).toBe(true);
  expect(shouldCalculateSizes("/home/me/projects/deep", "/home/me", settings)).toBe(true);
  expect(shouldCalculateSizes("/home/me-too", "/home/me", settings)).toBe(false);
  expect(shouldCalculateSizes("/home/me", "", settings)).toBe(false);
  expect(shouldCalculateSizes("/home/me", "/home/me", { ...settings, automaticSizesInHome: false })).toBe(false);
});

test("custom folders work independently and include descendants", () => {
  const settings = { automaticSizesInHome: false, automaticSizePaths: ["/data/work/", "/mnt/archive"] };
  expect(shouldCalculateSizes("/data/work/sub", "", settings)).toBe(true);
  expect(shouldCalculateSizes("/mnt/archive", "", settings)).toBe(true);
  expect(shouldCalculateSizes("/data/work-other", "", settings)).toBe(false);
});

test("handles roots, Windows separators and case, and POSIX case", () => {
  expect(isWithinFolder("/data", "/")).toBe(true);
  expect(isWithinFolder("C:\\Users\\Me\\Documents", "c:/users/me/")).toBe(true);
  expect(isWithinFolder("C:\\data", "C:\\")).toBe(true);
  expect(isWithinFolder("/Data", "/data")).toBe(false);
  expect(isWithinFolder("/data", "")).toBe(false);
});
