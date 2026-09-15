import { describe, expect, test } from "bun:test";
import { FOLDER_SORT_LIMIT, loadFolderSort, saveFolderSort } from "./folder-sort";

function memoryStorage() {
  const items = new Map<string, string>();
  return {
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
  };
}

describe("folder sort", () => {
  test("remembers the sort per folder", () => {
    const storage = memoryStorage();
    saveFolderSort("/Users/me/Downloads", { column: "date", dir: "desc" }, storage);
    saveFolderSort("/Users/me/Documents", { column: "name", dir: "asc" }, storage);
    expect(loadFolderSort("/Users/me/Downloads", storage)).toEqual({ column: "date", dir: "desc" });
    expect(loadFolderSort("/Users/me/Documents", storage)).toEqual({ column: "name", dir: "asc" });
    expect(loadFolderSort("/Users/me/Music", storage)).toBeNull();
  });

  test("a trailing separator names the same folder", () => {
    const storage = memoryStorage();
    saveFolderSort("s3://id/bucket/photos/", { column: "size", dir: "asc" }, storage);
    expect(loadFolderSort("s3://id/bucket/photos", storage)).toEqual({
      column: "size",
      dir: "asc",
    });
    saveFolderSort("/", { column: "type", dir: "desc" }, storage);
    expect(loadFolderSort("/", storage)).toEqual({ column: "type", dir: "desc" });
  });

  test("ignores malformed storage", () => {
    const storage = memoryStorage();
    storage.setItem("folder-sort", "not json");
    expect(loadFolderSort("/a", storage)).toBeNull();
    storage.setItem("folder-sort", JSON.stringify({ "/a": { column: "color", dir: "up" } }));
    expect(loadFolderSort("/a", storage)).toBeNull();
  });

  test("forgets the oldest folders past the limit", () => {
    const storage = memoryStorage();
    for (let index = 0; index <= FOLDER_SORT_LIMIT; index++) {
      saveFolderSort(`/folder-${index}`, { column: "name", dir: "desc" }, storage);
    }
    expect(loadFolderSort("/folder-0", storage)).toBeNull();
    expect(loadFolderSort(`/folder-${FOLDER_SORT_LIMIT}`, storage)).toEqual({
      column: "name",
      dir: "desc",
    });
  });
});
