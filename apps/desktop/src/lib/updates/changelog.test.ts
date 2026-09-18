import { describe, expect, it } from "vitest";
import { formatDate, latestVersion, noteLines, releaseFor, releases } from "./changelog.js";

describe("changelog", () => {
  it("loads the released versions, newest first", () => {
    expect(latestVersion).toMatch(/^\d+\.\d+\.\d+$/);
    expect(releases[0].version).toBe(latestVersion);
    expect(releases.length).toBeGreaterThan(1);
    expect(releases[0].groups.length).toBeGreaterThan(0);
  });

  it("finds a release by version", () => {
    expect(releaseFor("0.1.7")?.date).toBe("2026-09-17");
    expect(releaseFor("9.9.9")).toBeNull();
    expect(releaseFor(null)).toBeNull();
  });

  it("formats dates with Temporal", () => {
    expect(formatDate("2026-09-17")).toBe(
      Temporal.PlainDate.from("2026-09-17").toLocaleString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      }),
    );
    expect(formatDate(null)).toBe("");
    expect(formatDate("not a date")).toBe("not a date");
  });

  it("turns update notes into lines", () => {
    expect(noteLines("## 0.2.0\n- One.\n  - Two.\n\n")).toEqual(["## 0.2.0", "One.", "Two."]);
    expect(noteLines(null)).toEqual([]);
  });
});
