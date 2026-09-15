import { expect, test } from "bun:test";
import { downloadFraction, isPartialDownload } from "./download-progress.js";

test("temporary download names and honest progress for unknown totals", () => {
  for (const name of ["DaVinci_Resolve_21.1_Mac.zip.download", "file.CRDOWNLOAD", "movie.part"]) {
    expect(isPartialDownload(name)).toBe(true);
  }
  for (const name of ["file.zip", "download", "file.download.zip"]) {
    expect(isPartialDownload(name)).toBe(false);
  }
  expect(downloadFraction(251, 1000)).toBe(0.251);
  expect(downloadFraction(0, 1000)).toBe(0);
  expect(downloadFraction(1100, 1000)).toBe(1);
  for (const [done, total] of [[10, 0], [10, -1], [-1, 100], [NaN, 100], [10, Infinity]]) {
    expect(downloadFraction(done, total)).toBeNull();
  }
});
