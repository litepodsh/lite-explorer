import { describe, expect, test } from "bun:test";
import { canFavorite, dropIndex, moveTo } from "./favorites";

describe("dropIndex", () => {
  test("upper half inserts before the row", () => {
    expect(dropIndex(104, 100, 30, 2)).toBe(2);
  });

  test("lower half inserts after the row", () => {
    expect(dropIndex(120, 100, 30, 2)).toBe(3);
  });
});

describe("moveTo", () => {
  const items = ["a", "b", "c", "d"];

  test("moving down accounts for the removed item", () => {
    expect(moveTo(items, 0, 2)).toEqual(["b", "a", "c", "d"]);
  });

  test("moving up", () => {
    expect(moveTo(items, 3, 1)).toEqual(["a", "d", "b", "c"]);
  });

  test("dropping on its own slot keeps the order", () => {
    expect(moveTo(items, 1, 1)).toEqual(items);
    expect(moveTo(items, 1, 2)).toEqual(items);
  });

  test("to the start and the end", () => {
    expect(moveTo(items, 2, 0)).toEqual(["c", "a", "b", "d"]);
    expect(moveTo(items, 0, 4)).toEqual(["b", "c", "d", "a"]);
  });
});

describe("canFavorite", () => {
  test("local folder", () => {
    expect(canFavorite({ is_directory: true, path: "/Users/me/Downloads" })).toBe(true);
  });

  test("files are rejected but connected folders, buckets and shares are allowed", () => {
    expect(canFavorite({ is_directory: false, path: "/Users/me/notes.txt" })).toBe(false);
    expect(canFavorite({ is_directory: true, path: "s3://id/bucket/", kind: "bucket" })).toBe(
      true,
    );
    expect(canFavorite({ is_directory: true, path: "s3://id/bucket/folder/" })).toBe(true);
    expect(canFavorite({ is_directory: true, path: "smb://id/share", kind: "share" })).toBe(true);
    expect(canFavorite({ is_directory: true, path: "sftp://id/home" })).toBe(true);
  });
});
