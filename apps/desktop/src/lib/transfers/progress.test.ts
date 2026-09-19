import { describe, expect, test } from "bun:test";
import { etaSeconds, formatBytes, formatEta, progressFraction } from "./progress.js";

describe("progressFraction", () => {
  test("returns null when the total is unknown or zero", () => {
    expect(progressFraction(10, 0)).toBeNull();
    expect(progressFraction(0, -1)).toBeNull();
  });
  test("clamps to 0..1", () => {
    expect(progressFraction(5, 10)).toBe(0.5);
    expect(progressFraction(20, 10)).toBe(1);
    expect(progressFraction(-1, 10)).toBe(0);
  });
});

describe("etaSeconds", () => {
  test("returns null without a rate or a total", () => {
    expect(etaSeconds(0, 100, 5)).toBeNull();
    expect(etaSeconds(10, 0, 5)).toBeNull();
    expect(etaSeconds(10, 100, 0)).toBeNull();
    expect(etaSeconds(100, 100, 5)).toBeNull();
  });
  test("extrapolates remaining time from the observed rate", () => {
    expect(etaSeconds(50, 100, 10)).toBe(10);
  });
});

describe("formatEta", () => {
  test("formats seconds and minutes", () => {
    expect(formatEta(null)).toBeNull();
    expect(formatEta(0.4)).toBe("1s left");
    expect(formatEta(30)).toBe("30s left");
    expect(formatEta(90)).toBe("2m left");
  });
});

describe("formatBytes", () => {
  test("formats bytes, megabytes and gigabytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1_572_864)).toBe("1.5 MB");
    expect(formatBytes(2_147_483_648)).toBe("2.0 GB");
  });
});
