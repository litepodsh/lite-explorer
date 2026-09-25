import { describe, expect, test } from "bun:test";
import { buildIndex, resolve } from "./resolve.js";
import {
  COMPONENT_BINDINGS,
  MENU_ACCELERATORS,
  MENU_META,
  STANDARD_BINDINGS,
  YAZI_BINDINGS,
  allBindings,
  findConflicts,
  menuBindings,
  type Binding,
} from "./keymap.js";

const binding = (overrides: Partial<Binding>): Binding => ({
  keys: ["a"],
  scope: "list",
  mode: "standard",
  desc: "Test",
  category: "App",
  command: "test.a",
  ...overrides,
});

describe("menu bindings", () => {
  test("every menu accelerator has metadata", () => {
    expect(MENU_ACCELERATORS.filter(({ id }) => !MENU_META[id]).map(({ id }) => id)).toEqual([]);
  });

  test("menu items that an app binding runs are not listed twice", () => {
    const ids = menuBindings(STANDARD_BINDINGS).map((item) => item.menuId);
    expect(ids.includes("go-to-folder")).toBe(false);
    expect(ids.includes("new-tab-other")).toBe(false);
    expect(ids.includes("new-tab")).toBe(true);
  });

  test("menu bindings keep the menu accelerator keys", () => {
    const hidden = menuBindings(STANDARD_BINDINGS).find(
      (item) => item.menuId === "toggle-hidden-files",
    );
    expect(hidden?.keys).toEqual(["Mod+Shift+."]);
    expect(hidden?.source).toBe("menu");
  });
});

describe("findConflicts", () => {
  test("the shipped keymap has no conflicts", () => {
    expect(findConflicts(allBindings(), "macos")).toEqual([]);
    expect(findConflicts(allBindings(), "windows")).toEqual([]);
    expect(findConflicts(allBindings(), "linux")).toEqual([]);
  });

  test("reports the same keys twice in one scope and mode", () => {
    const result = findConflicts([binding({}), binding({ command: "test.b" })], "macos");
    expect(result).toEqual(["standard list: “a” is bound twice"]);
  });

  test("reports Mod and its platform key as the same keys", () => {
    const result = findConflicts(
      [binding({ keys: ["Mod+K"] }), binding({ keys: ["Meta+K"], command: "test.b" })],
      "macos",
    );
    expect(result).toEqual(["standard list: “Meta+K” is bound twice"]);
  });

  test("reports a single key that is also a chord prefix, including global chords", () => {
    const result = findConflicts(
      [binding({ keys: ["g"] }), binding({ keys: ["g", "h"], scope: "global", command: "test.b" })],
      "macos",
    );
    expect(result).toEqual(["standard list: “g” is also the start of a chord"]);
  });

  test("a yazi binding over a standard one needs overrides", () => {
    const standard = binding({ keys: ["<Space>"] });
    const yazi = binding({ keys: ["<Space>"], mode: "yazi", command: "test.b" });
    expect(findConflicts([standard, yazi], "macos")).toEqual([
      "yazi list: “<Space>” replaces a standard binding without overrides",
    ]);
    expect(findConflicts([standard, { ...yazi, overrides: true }], "macos")).toEqual([]);
  });

  test("overrides must replace something", () => {
    const yazi = binding({ keys: ["q"], mode: "yazi", overrides: true });
    expect(findConflicts([yazi], "macos")).toEqual(["yazi list: “q” overrides nothing"]);
  });

  test("bindings for other platforms don't conflict", () => {
    const mac = binding({ keys: ["Meta+<Backspace>"], platforms: ["macos"] });
    const other = binding({
      keys: ["Meta+<Backspace>"],
      platforms: ["windows"],
      command: "test.b",
    });
    expect(findConflicts([mac, other], "macos")).toEqual([]);
  });
});

describe("yazi and component bindings", () => {
  test("allBindings includes yazi and component bindings", () => {
    const all = allBindings();
    expect(all.some((item) => item.mode === "yazi" && item.keys.join(" ") === "g g")).toBe(true);
    expect(all.filter((item) => item.source === "component").length).toBe(
      COMPONENT_BINDINGS.length,
    );
  });

  test("yazi bindings never use keys the spec leaves out", () => {
    const forbidden = new Set([
      "q",
      "Q",
      "Ctrl+c",
      "Ctrl+z",
      ";",
      ":",
      "-",
      "_",
      "<Tab>",
      "Ctrl+f",
      "Ctrl+b",
      "P",
      "f",
      "n",
      "N",
      "O",
    ]);
    expect(
      YAZI_BINDINGS.filter((item) => forbidden.has(item.keys[0])).map((item) =>
        item.keys.join(" "),
      ),
    ).toEqual([]);
  });

  test("yazi select all exists only on macOS, pane right overrides elsewhere", () => {
    const selectAll = YAZI_BINDINGS.filter((item) => item.command === "selection.all");
    expect(selectAll.map((item) => item.platforms)).toEqual([["macos"]]);
    const paneRight = YAZI_BINDINGS.filter((item) => item.command === "pane.right");
    expect(paneRight.map((item) => Boolean(item.overrides))).toEqual([false, true]);
  });
});

describe("platform and text editing shortcuts", () => {
  test("Command+Backspace on macOS sends files to trash", () => {
    const index = buildIndex(allBindings(), "macos");
    const result = resolve({ index, scope: "list", mode: "standard", token: "Meta+<Backspace>",
      repeat: false, chord: null, now: 0, timeoutMs: 1500 });
    expect(result.kind === "run" && result.binding.command).toBe("file.trash");
  });

  test("pane shortcuts preserve cursor movement in text fields and code", () => {
    for (const platform of ["macos", "windows", "linux"] as const) {
      const index = buildIndex(allBindings(), platform);
      for (const scope of ["input", "monaco"] as const) {
        for (const direction of ["Left", "Right"]) {
          const token = `${platform === "macos" ? "Meta" : "Ctrl"}+<${direction}>`;
          expect(resolve({ index, scope, mode: "standard", token,
            repeat: false, chord: null, now: 0, timeoutMs: 1500 }).kind).toBe("none");
        }
      }
    }
  });
});
