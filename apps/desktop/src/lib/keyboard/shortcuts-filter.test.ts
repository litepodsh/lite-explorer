import { describe, expect, test } from "bun:test";
import type { Binding } from "./keymap.js";
import { descMatch, filterShortcuts, normalizeQuery, shortcutRows } from "./shortcuts-filter.js";

const b = (keys: string[], desc: string, extra: Partial<Binding> = {}): Binding => ({
  keys,
  desc,
  command: desc,
  scope: "list",
  mode: "yazi",
  category: "Navigation",
  ...extra,
});

const bindings: Binding[] = [
  b(["g", "g"], "First item"),
  b(["G"], "Last item"),
  b(["g", "h"], "Go home", { scope: "global" }),
  b(["Ctrl+d"], "Down half a page"),
  b(["Mod+Shift+P"], "Command palette", { mode: "standard", scope: "global", category: "App" }),
  b(["Shift+<Delete>"], "Delete permanently", { mode: "standard", category: "Files" }),
  b(["Meta+<Backspace>"], "Delete permanently", {
    mode: "standard",
    category: "Files",
    platforms: ["macos"],
  }),
  b(["<Tab>"], "Unrelated tab key", { mode: "standard" }),
];

describe("shortcutRows", () => {
  test("merges keys of the same command and skips other platforms", () => {
    const mac = shortcutRows(bindings, "macos").find((row) => row.desc === "Delete permanently");
    expect(mac?.sequences).toEqual([["Shift+<Delete>"], ["Meta+<Backspace>"]]);
    const windows = shortcutRows(bindings, "windows").find(
      (row) => row.desc === "Delete permanently",
    );
    expect(windows?.sequences).toEqual([["Shift+<Delete>"]]);
  });
});

describe("normalizeQuery", () => {
  test("accepts chords with or without spaces", () => {
    expect(normalizeQuery("g g", "macos")).toBe("gg");
    expect(normalizeQuery("gg", "macos")).toBe("gg");
  });

  test("accepts modifier words, symbols and vim notation", () => {
    expect(normalizeQuery("ctrl+d", "macos")).toBe("Ctrl+D");
    expect(normalizeQuery("<C-d>", "macos")).toBe("Ctrl+D");
    expect(normalizeQuery("⇧⌘P", "macos")).toBe("Shift+Meta+P");
    expect(normalizeQuery("cmd shift p", "windows")).toBe("Ctrl+Shift+P");
    expect(normalizeQuery("Shift+Del", "linux")).toBe("Shift+<Delete>");
    expect(normalizeQuery("shift g", "macos")).toBe("G");
  });

  test("leaves plain words alone", () => {
    expect(normalizeQuery("home", "macos")).toBe("<Home>");
    expect(normalizeQuery("copy", "macos")).toBe("copy");
  });
});

describe("filterShortcuts", () => {
  const rows = shortcutRows(bindings, "macos");
  const descs = (query: string) => filterShortcuts(rows, query, "macos").map((row) => row.desc);

  test("empty query returns every row", () => {
    expect(filterShortcuts(rows, "  ", "macos").length).toBe(rows.length);
  });

  test("exact key first, then key prefix, then descriptions", () => {
    expect(descs("G")).toEqual(["Last item", "First item", "Go home", "Down half a page"]);
    expect(descs("g")).toEqual(["First item", "Last item", "Go home", "Down half a page"]);
  });

  test("modifier queries match exactly", () => {
    expect(descs("⇧⌘P")).toEqual(["Command palette"]);
    expect(descs("<C-d>")).toEqual(["Down half a page"]);
  });

  test("keys rank before descriptions", () => {
    expect(descs("tab")).toEqual(["Unrelated tab key"]);
    expect(descs("half")).toEqual(["Down half a page"]);
  });

  test("fuzzy matches come last and only for rows not matched yet", () => {
    const fuzzy = (rest: typeof rows) => rest.filter((row) => row.desc === "Go home");
    expect(filterShortcuts(rows, "hme", "macos", fuzzy).map((row) => row.desc)).toEqual([
      "Go home",
    ]);
  });
});

describe("descMatch", () => {
  test("returns the first case-insensitive match range", () => {
    expect(descMatch("Go home", "HOME")).toEqual([[3, 6]]);
    expect(descMatch("Go home", "x")).toBeUndefined();
  });
});
