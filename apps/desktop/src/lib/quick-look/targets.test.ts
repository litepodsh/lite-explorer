import { describe, expect, test } from "bun:test";
import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";
import { quickLookTargets } from "./targets.js";

const file = (path: string): DirectoryEntry => ({
  name: path.split("/").at(-1) ?? path,
  path,
  is_directory: false,
  is_hidden: false,
});

describe("quickLookTargets", () => {
  test("starts at the focused row of the selection", () => {
    expect(quickLookTargets([file("/a"), file("/b"), file("/c")], "/b")).toEqual({
      paths: ["/a", "/b", "/c"],
      index: 1,
    });
  });

  test("starts at the first item when focus is elsewhere", () => {
    expect(quickLookTargets([file("/a")], "/z").index).toBe(0);
  });

  test("leaves out remote rows", () => {
    expect(quickLookTargets([file("s3://bucket/a.txt"), file("/b")], "/b").paths).toEqual(["/b"]);
  });
});
