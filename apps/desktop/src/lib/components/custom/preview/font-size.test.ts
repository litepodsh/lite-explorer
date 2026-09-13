import { describe, expect, test } from "bun:test";
import {
  clampFontSize,
  DEFAULT_FONT_SIZE,
  fontSizeForShortcut,
  lineHeightFor,
  parseFontSize,
} from "./font-size.js";

describe("fontSizeForShortcut", () => {
  test("steps down, up and resets", () => {
    expect(fontSizeForShortcut(12, "-")).toBe(11);
    expect(fontSizeForShortcut(12, "=")).toBe(13);
    expect(fontSizeForShortcut(12, "+")).toBe(13);
    expect(fontSizeForShortcut(17, "0")).toBe(DEFAULT_FONT_SIZE);
  });

  test("stays within 8 and 24", () => {
    expect(fontSizeForShortcut(8, "-")).toBe(8);
    expect(fontSizeForShortcut(24, "=")).toBe(24);
  });

  test("ignores other keys", () => {
    expect(fontSizeForShortcut(12, "f")).toBeNull();
  });
});

describe("clampFontSize and parseFontSize", () => {
  test("clamps and rounds", () => {
    expect(clampFontSize(3)).toBe(8);
    expect(clampFontSize(40)).toBe(24);
    expect(clampFontSize(12.6)).toBe(13);
    expect(clampFontSize(Number.NaN)).toBe(DEFAULT_FONT_SIZE);
  });

  test("parses saved values", () => {
    expect(parseFontSize(null)).toBe(DEFAULT_FONT_SIZE);
    expect(parseFontSize("")).toBe(DEFAULT_FONT_SIZE);
    expect(parseFontSize("15")).toBe(15);
    expect(parseFontSize("abc")).toBe(DEFAULT_FONT_SIZE);
  });

  test("line height is 1.5x rounded", () => {
    expect(lineHeightFor(12)).toBe(18);
    expect(lineHeightFor(13)).toBe(20);
  });
});
