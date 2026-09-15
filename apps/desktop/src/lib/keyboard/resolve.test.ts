import { describe, expect, test } from "bun:test";
import type { Binding } from "./keymap.js";
import { buildIndex, resolve, type ResolveInput } from "./resolve.js";

const b = (keys: string[], command: string, overrides: Partial<Binding> = {}): Binding => ({
  keys,
  command,
  scope: "list",
  mode: "standard",
  desc: command,
  category: "App",
  ...overrides,
});

const bindings: Binding[] = [
  b(["<Down>"], "list.next", { modifiers: ["Shift", "Mod"] }),
  b(["<Space>"], "selection.only"),
  b(["<Space>"], "selection.toggleNext", { mode: "yazi", overrides: true }),
  b(["j"], "list.next.yazi", { mode: "yazi" }),
  b(["g", "g"], "list.top", { mode: "yazi" }),
  b(["g", "h"], "go.home", { mode: "yazi", scope: "global" }),
  b(["Mod+F"], "search.focus", { scope: "global", where: ["input"] }),
  b(["Mod+T"], "tab.new", { scope: "global", source: "menu" }),
  b(["<Delete>"], "file.trash", { repeat: false }),
  b(["Meta+<Backspace>"], "file.mac", { platforms: ["macos"] }),
];

const input = (overrides: Partial<ResolveInput>): ResolveInput => ({
  index: buildIndex(bindings, "macos"),
  scope: "list",
  mode: "standard",
  token: "<Down>",
  repeat: false,
  chord: null,
  now: 1000,
  timeoutMs: 1500,
  ...overrides,
});

const commandOf = (result: ReturnType<typeof resolve>) =>
  result.kind === "run" ? result.binding.command : result.kind;

describe("resolve", () => {
  test("runs a single-key match with empty args", () => {
    const result = resolve(input({}));
    expect(commandOf(result)).toBe("list.next");
    expect(result.kind === "run" ? result.args : null).toEqual({});
  });

  test("strips accepted modifiers and passes them as args", () => {
    const result = resolve(input({ token: "Shift+Meta+<Down>" }));
    expect(commandOf(result)).toBe("list.next");
    expect(result.kind === "run" ? result.args : null).toEqual({ shift: true, primary: true });
  });

  test("does not strip modifiers the binding doesn't accept", () => {
    expect(resolve(input({ token: "Alt+<Down>" })).kind).toBe("none");
    expect(resolve(input({ token: "Ctrl+<Down>" })).kind).toBe("none");
  });

  test("yazi bindings are ignored in standard mode", () => {
    expect(resolve(input({ token: "j" })).kind).toBe("none");
  });

  test("yazi overrides replace the standard binding", () => {
    expect(commandOf(resolve(input({ token: "<Space>" })))).toBe("selection.only");
    expect(commandOf(resolve(input({ token: "<Space>", mode: "yazi" })))).toBe(
      "selection.toggleNext",
    );
  });

  test("global bindings reach restricted scopes only through where", () => {
    expect(commandOf(resolve(input({ token: "Meta+F", scope: "input" })))).toBe("search.focus");
    expect(resolve(input({ token: "Meta+F", scope: "dialog" })).kind).toBe("none");
    expect(resolve(input({ token: "<Down>", scope: "input" })).kind).toBe("none");
  });

  test("menu bindings never run", () => {
    expect(resolve(input({ token: "Meta+T", scope: "global" })).kind).toBe("none");
  });

  test("repeat false ignores held keys", () => {
    expect(resolve(input({ token: "<Delete>", repeat: true })).kind).toBe("none");
    expect(commandOf(resolve(input({ token: "<Delete>" })))).toBe("file.trash");
  });

  test("platform-only bindings are indexed only on that platform", () => {
    expect(commandOf(resolve(input({ token: "Meta+<Backspace>" })))).toBe("file.mac");
    const windows = input({ index: buildIndex(bindings, "windows"), token: "Meta+<Backspace>" });
    expect(resolve(windows).kind).toBe("none");
  });

  test("a chord prefix waits, gathering scope and global candidates", () => {
    const first = resolve(input({ token: "g", mode: "yazi" }));
    expect(first.kind).toBe("pending");
    const chord = first.kind === "pending" ? first.chord : null;
    expect(chord?.typed).toEqual(["g"]);
    expect(chord?.candidates.map((item) => item.command)).toEqual(["list.top", "go.home"]);
  });

  test("the second key completes the chord", () => {
    const first = resolve(input({ token: "g", mode: "yazi" }));
    const chord = first.kind === "pending" ? first.chord : null;
    expect(commandOf(resolve(input({ token: "h", mode: "yazi", chord, now: 1500 })))).toBe(
      "go.home",
    );
  });

  test("Escape or an unknown second key cancels", () => {
    const first = resolve(input({ token: "g", mode: "yazi" }));
    const chord = first.kind === "pending" ? first.chord : null;
    expect(resolve(input({ token: "<Esc>", mode: "yazi", chord })).kind).toBe("cancel");
    expect(resolve(input({ token: "x", mode: "yazi", chord })).kind).toBe("cancel");
  });

  test("an expired chord is ignored and the key resolves on its own", () => {
    const first = resolve(input({ token: "g", mode: "yazi" }));
    const chord = first.kind === "pending" ? first.chord : null;
    expect(commandOf(resolve(input({ token: "j", mode: "yazi", chord, now: 2600 })))).toBe(
      "list.next.yazi",
    );
  });
});
