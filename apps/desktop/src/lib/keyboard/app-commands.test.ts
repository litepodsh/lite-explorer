import { describe, expect, test } from "bun:test";
import { CommandRegistry } from "./commands.js";
import { standardCommands } from "./app-commands.js";
import type { AppContext } from "./context.js";
import { fakeContext } from "./test-context.js";

type Calls = string[];
import { STANDARD_BINDINGS } from "./keymap.js";

function setup() {
  const registry = new CommandRegistry<AppContext>();
  registry.register(standardCommands());
  return registry;
}

describe("standardCommands", () => {
  test("every standard binding points at a registered command", () => {
    const registry = setup();
    const missing = STANDARD_BINDINGS.filter((binding) => binding.command && !registry.has(binding.command)).map((binding) => binding.command);
    expect(missing).toEqual([]);
  });

  test("navigation passes modifiers", () => {
    const calls: Calls = [];
    setup().run("list.next", fakeContext(calls), { shift: true, primary: false });
    expect(calls).toEqual(["move ArrowDown true false"]);
  });

  test("list commands wait while the list is blocked", () => {
    const calls: Calls = [];
    const result = setup().run("list.next", fakeContext(calls, { list: { blocked: () => true } }), {});
    expect(result).toBe(false);
    expect(calls).toEqual([]);
  });

  test("Left opens the parent in list view and moves in icon view", () => {
    const calls: Calls = [];
    const registry = setup();
    registry.run("list.left", fakeContext(calls), {});
    registry.run("list.left", fakeContext(calls, { list: { view: () => "grid" } }), { shift: true });
    expect(calls).toEqual(["parent", "move ArrowLeft true false"]);
  });

  test("Right with modifiers in list view does nothing", () => {
    const calls: Calls = [];
    expect(setup().run("list.right", fakeContext(calls), { shift: true })).toBe(false);
    expect(calls).toEqual([]);
  });

  test("Escape leaves checkbox mode first", () => {
    const calls: Calls = [];
    setup().run("app.escape", fakeContext(calls, { checkboxes: { active: () => true, exit: () => void calls.push("exit checkboxes") } }), {});
    expect(calls).toEqual(["exit checkboxes", "clear"]);
  });

  test("Escape clears the selection only from the list", () => {
    const calls: Calls = [];
    const registry = setup();
    expect(registry.run("app.escape", fakeContext(calls, { scope: "global" }), {})).toBe(false);
    registry.run("app.escape", fakeContext(calls), {});
    expect(calls).toEqual(["clear"]);
  });

  test("paste needs something to paste", () => {
    const calls: Calls = [];
    expect(setup().run("file.paste", fakeContext(calls, { list: { canPaste: () => false } }), {})).toBe(false);
    expect(calls).toEqual([]);
  });

  test("search focus leaves Mod+F to a focused archive view", () => {
    const calls: Calls = [];
    const registry = setup();
    expect(registry.run("search.focus", fakeContext(calls, { focusInside: () => true }), {})).toBe(false);
    registry.run("search.focus", fakeContext(calls), {});
    expect(calls).toEqual(["search"]);
  });

  test("pane focus stays inside the pane count", () => {
    const calls: Calls = [];
    const registry = setup();
    expect(registry.run("pane.left", fakeContext(calls), {})).toBe(false);
    registry.run("pane.right", fakeContext(calls), {});
    expect(calls).toEqual(["pane 1"]);
  });

  test("tab digits report whether a tab exists", () => {
    const calls: Calls = [];
    const registry = setup();
    expect(registry.run("tab.goto", fakeContext(calls), { digit: 2 })).toBe(true);
    expect(registry.run("tab.goto", fakeContext(calls), { digit: 7 })).toBe(false);
  });
});
