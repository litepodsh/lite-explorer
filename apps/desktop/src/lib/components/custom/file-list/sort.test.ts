import { describe, expect, it } from "bun:test";
import { orderEntries, sortEntries, type SortColumn } from "./sort.js";
import type { DirectoryEntry } from "./list-item.svelte";

function entry(name: string, is_directory: boolean, size = 0): DirectoryEntry {
  return { name, path: `/${name}`, is_directory, size, is_hidden: name.startsWith(".") };
}

function names(sorted: DirectoryEntry[]): string[] {
  return sorted.map((entry) => entry.name);
}

describe("sortEntries", () => {
  const files: DirectoryEntry[] = [
    entry("banana.txt", false, 300),
    entry("Apple.txt", false, 100),
    entry("docs", true, 0),
    entry("zebra.log", false, 200),
    entry("assets", true, 0),
  ];

  it("sorts by name ascending (case-insensitive)", () => {
    expect(names(sortEntries(files, "name", "asc"))).toEqual([
      "Apple.txt",
      "assets",
      "banana.txt",
      "docs",
      "zebra.log",
    ]);
  });

  it("sorts by name descending", () => {
    expect(names(sortEntries(files, "name", "desc"))).toEqual([
      "zebra.log",
      "docs",
      "banana.txt",
      "assets",
      "Apple.txt",
    ]);
  });

  it("sorts by type ascending (files before folders: \"File\" < \"Folder\")", () => {
    expect(names(sortEntries(files, "type", "asc"))).toEqual([
      "banana.txt",
      "Apple.txt",
      "zebra.log",
      "docs",
      "assets",
    ]);
  });

  it("sorts by type descending (folders before files)", () => {
    expect(names(sortEntries(files, "type", "desc"))).toEqual([
      "docs",
      "assets",
      "banana.txt",
      "Apple.txt",
      "zebra.log",
    ]);
  });

  it("sorts by size ascending (unmeasured folders count as 0)", () => {
    expect(names(sortEntries(files, "size", "asc"))).toEqual([
      "docs",
      "assets",
      "Apple.txt",
      "zebra.log",
      "banana.txt",
    ]);
  });

  it("sorts by size descending", () => {
    expect(names(sortEntries(files, "size", "desc"))).toEqual([
      "banana.txt",
      "zebra.log",
      "Apple.txt",
      "docs",
      "assets",
    ]);
  });

  it("does not mutate the input array", () => {
    const input = [entry("b", false, 2), entry("a", false, 1)];
    sortEntries(input, "name", "asc");
    expect(input.map((entry) => entry.name)).toEqual(["b", "a"]);
  });
});

it("sorts by size descending, mixing folders and files", () => {
  const input = [
    entry("small folder", true, 2),
    entry("huge file", false, 1000),
    entry("large folder", true, 30),
    entry("small file", false, 1),
  ];
  expect(names(sortEntries(input, "size", "desc"))).toEqual([
    "huge file",
    "large folder",
    "small folder",
    "small file",
  ]);
});

describe("orderEntries", () => {
  const first = [entry("b.txt", false, 2), entry("docs", true), entry(".env", false, 1)];
  const added = [entry("a.txt", false, 2), entry("assets", true), entry("c.txt", false, 3)];

  it("matches a full sort when rows are appended", () => {
    for (const column of ["name", "size", "type", null] as (SortColumn | null)[]) {
      for (const showHidden of [true, false]) {
        const previous = orderEntries(first, column, "asc", showHidden, null);
        const all = [...first, ...added];
        const incremental = orderEntries(all, column, "asc", showHidden, previous).shown;
        const full = orderEntries(all, column, "asc", showHidden, null).shown;
        expect(names(incremental)).toEqual(names(full));
      }
    }
  });

  it("sorts from scratch when rows change in place", () => {
    const previous = orderEntries(first, "size", "desc", true, null);
    const changed = [{ ...first[0], size: 0 }, first[1], first[2]];
    expect(names(orderEntries(changed, "size", "desc", true, previous).shown)).toEqual([
      ".env",
      "b.txt",
      "docs",
    ]);
  });

  it("reuses the previous order for the same listing", () => {
    const previous = orderEntries(first, "name", "asc", true, null);
    expect(orderEntries(first, "name", "asc", true, previous)).toBe(previous);
  });
});
