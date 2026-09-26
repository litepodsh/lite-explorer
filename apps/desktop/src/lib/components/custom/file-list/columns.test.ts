import { describe, expect, test } from "bun:test";
import { DEFAULT_COLUMNS, loadColumns, saveColumns, moveColumn } from "./columns";
import { sortEntries } from "./sort";

describe("folder column order", () => {
  const memory = () => {
    const values = new Map<string, string>();
    return {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => {
        values.set(key, value);
      },
    };
  };
  test("persists independently by folder without changing sort preferences", () => {
    const storage = memory();
    const order = moveColumn(DEFAULT_COLUMNS, "modified", "name");
    saveColumns("/photos/", order, storage);
    expect(loadColumns("/photos", storage)).toEqual(order);
    expect(loadColumns("/documents", storage)).toEqual(DEFAULT_COLUMNS);
    expect(storage.getItem("folder-sort")).toBeNull();
  });
  test("repairs duplicate, missing and invalid columns", () => {
    const storage = memory();
    storage.setItem(
      "folder-column-order",
      JSON.stringify({ "/a": ["size", "size", "bogus", "name"] }),
    );
    expect(loadColumns("/a", storage)).toEqual(["size", "name", "type", "date", "modified"]);
    storage.setItem("folder-column-order", "broken");
    expect(loadColumns("/a", storage)).toEqual(DEFAULT_COLUMNS);
  });
  test("creation and modification sort independently", () => {
    const a = {
      name: "a",
      path: "/a",
      is_directory: false,
      is_hidden: false,
      created: 1,
      modified: 20,
    };
    const b = { ...a, name: "b", path: "/b", created: 2, modified: 10 };
    expect(sortEntries([a, b], "date", "asc").map((entry) => entry.name)).toEqual(["a", "b"]);
    expect(sortEntries([a, b], "modified", "asc").map((entry) => entry.name)).toEqual(["b", "a"]);
  });
});
