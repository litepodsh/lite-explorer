import { describe, expect, it } from "vitest";
import { parseChangelog } from "./parse-changelog.js";

const source = `# Changelog

Intro text that must be ignored.

## [Unreleased]

## [0.2.0] - 2026-10-01

### Added

- Tabs in two panes.
-   Nested bullet.

### Fixed

- Crash on quit.

## [0.1.0] - 2026-09-13

Loose bullet without a group.

- First release.
`;

describe("parseChangelog", () => {
  it("reads version, date and groups", () => {
    const releases = parseChangelog(source);
    expect(releases.map((release) => release.version)).toEqual(["0.2.0", "0.1.0"]);
    expect(releases[0].date).toBe("2026-10-01");
    expect(releases[0].groups).toEqual([
      { kind: "added", label: "Added", items: ["Tabs in two panes.", "Nested bullet."] },
      { kind: "fixed", label: "Fixed", items: ["Crash on quit."] },
    ]);
  });

  it("groups bullets written without a heading under Changed", () => {
    const releases = parseChangelog(source);
    expect(releases[1].groups).toEqual([
      { kind: "changed", label: "Changed", items: ["First release."] },
    ]);
  });

  it("drops release sections with nothing in them", () => {
    expect(parseChangelog("## [Unreleased]\n")).toEqual([]);
  });

  it("reads a version without a date", () => {
    const [release] = parseChangelog("## 0.3.0\n\n### Added\n- Dark mode.\n");
    expect(release).toMatchObject({ version: "0.3.0", date: null });
  });
});
