import { describe, expect, test } from "bun:test";
import type { ArchiveEntry } from "$lib/file-ops/archive.js";
import { formatSize } from "$lib/components/custom/preview/format.js";
import {
  collapseSelection,
  dragIcon,
  dragLabel,
  fromTreePath,
  sizeByTreePath,
  summaryLine,
  toTreePaths,
} from "./extraction.js";

const entry = (path: string, isDirectory = false, size = 0, skipped = false): ArchiveEntry => ({
  path,
  isDirectory,
  size,
  skipped,
});

const listed = [entry("src", true), entry("src/a.ts", false, 3), entry("../x", false, 1, true)];

describe("tree paths", () => {
  test("marks directories with a trailing slash and leaves out skipped entries", () => {
    expect(toTreePaths(listed)).toEqual(["src/", "src/a.ts"]);
  });

  test("converts tree paths back to archive paths", () => {
    expect(fromTreePath("src/lib/")).toBe("src/lib");
    expect(fromTreePath("a.ts")).toBe("a.ts");
  });

  test("maps file sizes by tree path", () => {
    expect(sizeByTreePath(listed)).toEqual(new Map([["src/a.ts", 3]]));
  });
});

describe("collapseSelection", () => {
  test("drops entries inside a selected folder but keeps look-alike siblings", () => {
    expect(collapseSelection(["src/lib/", "src/lib/a.ts", "src/libs/b.ts", "README.md"])).toEqual([
      "README.md",
      "src/lib",
      "src/libs/b.ts",
    ]);
  });
});

describe("labels", () => {
  test("summarizes item count, size and skipped entries", () => {
    expect(summaryLine({ entries: listed, uncompressedSize: 3, truncated: false })).toBe(
      `2 items · ${formatSize(3)} uncompressed · 1 skipped`,
    );
    expect(summaryLine({ entries: [entry("a.txt", false, 1)], uncompressedSize: 1, truncated: false })).toBe(
      `1 item · ${formatSize(1)} uncompressed`,
    );
  });

  test("names one dragged item or counts several", () => {
    expect(dragLabel(["src/lib/"])).toBe("lib");
    expect(dragLabel(["a.ts", "b.ts", "c/"])).toBe("3 items");
  });

  test("shows a file icon only for a single file", () => {
    expect(dragIcon(["a.ts"])).toBe("file");
    expect(dragIcon(["src/"])).toBe("folder");
    expect(dragIcon(["a.ts", "b.ts"])).toBe("folder");
  });
});
