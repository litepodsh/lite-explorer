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

  it("pins active downloads to the top, ahead of the sorted order", () => {
    const list = [entry("zebra", false, 1), entry("apple", false, 2), entry("mango", false, 3)];
    const active = new Set(["/mango"]);
    expect(names(orderEntries(list, "name", "asc", true, null, active).shown)).toEqual([
      "mango",
      "apple",
      "zebra",
    ]);
    // Descending name still has the active download first.
    expect(names(orderEntries(list, "name", "desc", true, null, active).shown)).toEqual([
      "mango",
      "zebra",
      "apple",
    ]);
  });

  it("pins multiple active downloads in their relative source order", () => {
    const list = [entry("zebra", false, 1), entry("apple", false, 2), entry("mango", false, 3), entry("kiwi", false, 4)];
    const active = new Set(["/zebra", "/kiwi"]);
    expect(names(orderEntries(list, "name", "asc", true, null, active).shown)).toEqual([
      "zebra",
      "kiwi",
      "apple",
      "mango",
    ]);
  });

  it("falls back to the normal order when nothing is downloading", () => {
    const list = [entry("zebra", false, 1), entry("apple", false, 2)];
    expect(names(orderEntries(list, "name", "asc", true, null, new Set()).shown)).toEqual([
      "apple",
      "zebra",
    ]);
  });

  it("updates the pinned block when a download finishes mid-streaming", () => {
    const initial = [entry("b", false, 2), entry("a", false, 1)];
    const previous = orderEntries(initial, "name", "asc", true, null, new Set(["/b"]));
    // "b" finished downloading: no longer in active set.
    const next = [...initial, entry("c", false, 3)];
    const shown = orderEntries(next, "name", "asc", true, previous, new Set()).shown;
    expect(names(shown)).toEqual(["a", "b", "c"]);
  });

  it("pins partial downloads (.crdownload/.part/.download) alongside active transfers", () => {
    const list = [
      entry("zebra.zip", false, 1),
      entry("chrome.crdownload", false, 2),
      entry("apple.zip", false, 3),
      entry("firefox.part", false, 4),
      entry("mango.download", false, 5),
    ];
    const partial = new Set(["/chrome.crdownload", "/firefox.part", "/mango.download"]);
    expect(names(orderEntries(list, "name", "asc", true, null, undefined, partial).shown)).toEqual([
      "chrome.crdownload",
      "firefox.part",
      "mango.download",
      "apple.zip",
      "zebra.zip",
    ]);
  });

  it("mixes active and partial into a single pinned block, preserving source order", () => {
    const list = [
      entry("zebra.zip", false, 1),
      entry("chrome.crdownload", false, 2),
      entry("apple.zip", false, 3),
      entry("firefox.part", false, 4),
    ];
    const active = new Set(["/apple.zip"]);
    const partial = new Set(["/chrome.crdownload", "/firefox.part"]);
    // Source order in the pinned block: chrome.crdownload (idx 1), apple.zip (idx 2 active),
    // firefox.part (idx 3). The remaining "rest" sorts as usual.
    expect(names(orderEntries(list, "name", "asc", true, null, active, partial).shown)).toEqual([
      "chrome.crdownload",
      "apple.zip",
      "firefox.part",
      "zebra.zip",
    ]);
  });

  it("partial-only still pins to the top, with the normal sort below", () => {
    const list = [entry("zebra.zip", false, 1), entry("apple.zip", false, 2), entry("mango.zip", false, 3)];
    const partial = new Set(["/mango.zip"]);
    expect(names(orderEntries(list, "name", "desc", true, null, undefined, partial).shown)).toEqual([
      "mango.zip",
      "zebra.zip",
      "apple.zip",
    ]);
  });

  it("a partial file renamed to a final name drops out of the pin on the next tick", () => {
    const initial = [
      entry("b.zip", false, 2),
      entry("a.crdownload", false, 1),
    ];
    const previous = orderEntries(initial, "name", "asc", true, null, undefined, new Set(["/a.crdownload"]));
    expect(names(previous.shown)).toEqual(["a.crdownload", "b.zip"]);
    // Listing refresh: the .crdownload was renamed to its final .zip name, no longer partial.
    const next = [entry("b.zip", false, 2), entry("a.zip", false, 1)];
    const shown = orderEntries(next, "name", "asc", true, previous, undefined, new Set()).shown;
    expect(names(shown)).toEqual(["a.zip", "b.zip"]);
  });
});
