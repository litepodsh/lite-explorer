import { describe, expect, test } from "bun:test";
import type { DirectoryEntry } from "#lib/components/custom/file-list/index.js";
import { applyListingChanges, mergeListing, removePaths, renameEntry, upsertEntry } from "./listing-merge.js";

function entry(path: string, extra: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    name: path.split("/").at(-1) ?? path,
    path,
    is_directory: false,
    is_hidden: false,
    ...extra,
  };
}

describe("mergeListing", () => {
  test("keeps measured sizes of entries that are still listed", () => {
    const previous = [entry("/a/big", { is_directory: true, size: 500, sizeComplete: true })];
    const next = [entry("/a/big", { is_directory: true }), entry("/a/new.txt", { size: 10 })];

    expect(mergeListing(previous, next)).toEqual([
      entry("/a/big", { is_directory: true, size: 500, sizeComplete: true }),
      entry("/a/new.txt", { size: 10 }),
    ]);
  });

  test("keeps a partial size and its incomplete flag", () => {
    const previous = [entry("/a/part", { is_directory: true, size: 12, sizeComplete: false })];
    const next = [entry("/a/part", { is_directory: true, size: 99 })];

    expect(mergeListing(previous, next)[0]).toMatchObject({ size: 12, sizeComplete: false });
  });

  test("drops sizes of entries that disappeared", () => {
    const previous = [entry("/a/gone", { is_directory: true, size: 40, sizeComplete: true })];
    const next = [entry("/a/kept.txt")];

    expect(mergeListing(previous, next)).toEqual(next);
  });

  test("returns the new listing untouched when nothing was measured", () => {
    const next = [entry("/a/one.txt"), entry("/a/two.txt")];

    expect(mergeListing([entry("/a/old.txt")], next)).toBe(next);
  });

  test("takes the fresh metadata of a changed entry", () => {
    const previous = [entry("/a/one.txt", { size: 5, sizeComplete: true })];
    const next = [entry("/a/one.txt", { size: 900 })];

    expect(mergeListing(previous, next)[0]).toMatchObject({ size: 5, sizeComplete: true });
  });
});

describe("removePaths", () => {
  test("removes only the given paths", () => {
    const entries = [entry("/a/1"), entry("/a/2"), entry("/a/3")];

    expect(removePaths(entries, ["/a/2"]).map((item) => item.path)).toEqual(["/a/1", "/a/3"]);
  });

  test("returns the same array when nothing matches", () => {
    const entries = [entry("/a/1")];

    expect(removePaths(entries, [])).toBe(entries);
  });
});

describe("upsertEntry", () => {
  test("appends an unseen entry and replaces a listed one", () => {
    const created = entry("/a/new", { size: 3 });

    expect(upsertEntry([entry("/a/1")], created).map((item) => item.path)).toEqual([
      "/a/1",
      "/a/new",
    ]);
    expect(upsertEntry([entry("/a/new")], created)).toEqual([created]);
  });
});

describe("renameEntry", () => {
  test("renames in place and keeps the measured size", () => {
    const entries = [
      entry("/a/one.txt"),
      entry("/a/folder", { is_directory: true, size: 70, sizeComplete: true }),
    ];

    const renamed = renameEntry(entries, "/a/folder", "/a/renamed", "renamed");

    expect(renamed.map((item) => item.path)).toEqual(["/a/one.txt", "/a/renamed"]);
    expect(renamed[1]).toMatchObject({ name: "renamed", size: 70, sizeComplete: true });
  });

  test("ignores a path that isn’t listed", () => {
    const entries = [entry("/a/1")];

    expect(renameEntry(entries, "/a/missing", "/a/other", "other")).toBe(entries);
  });
});

describe("applyListingChanges", () => {
  const file = (name: string, size = 1): DirectoryEntry => ({
    name,
    path: `/f/${name}`,
    is_directory: false,
    is_hidden: false,
    size,
  });

  test("drops gone rows, replaces changed ones and appends new ones", () => {
    const next = applyListingChanges(
      [file("a"), file("b"), file("c")],
      [
        { path: "/f/b", entry: null },
        { path: "/f/c", entry: file("c", 9) },
        { path: "/f/d", entry: file("d") },
        { path: "/f/never", entry: null },
      ],
    );
    expect(next.map((entry) => [entry.name, entry.size])).toEqual([
      ["a", 1],
      ["c", 9],
      ["d", 1],
    ]);
  });

  test("keeps a measured folder size", () => {
    const folder = { ...file("dir"), is_directory: true, size: 500, sizeComplete: true };
    const fresh = { ...file("dir"), is_directory: true, size: undefined, modified: 2 };
    const [next] = applyListingChanges([folder], [{ path: "/f/dir", entry: fresh }]);
    expect(next).toMatchObject({ size: 500, sizeComplete: true, modified: 2 });
  });

  test("returns the same listing when nothing changed", () => {
    const entries = [file("a")];
    expect(applyListingChanges(entries, [])).toBe(entries);
  });
});
