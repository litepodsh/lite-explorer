import { describe, expect, test } from "bun:test";
import { CommandRegistry } from "./commands.js";
import { standardCommands } from "./app-commands.js";
import { navigationCommands } from "./navigation-commands.js";
import type { AppContext } from "./context.js";
import { APP_BINDINGS } from "./keymap.js";
import { fakeContext } from "./test-context.js";

function setup() {
  const registry = new CommandRegistry<AppContext>();
  registry.register([...standardCommands(), ...navigationCommands()]);
  return registry;
}

describe("navigationCommands", () => {
  test("every app binding points at a registered command", () => {
    const registry = setup();
    const missing = APP_BINDINGS.filter((item) => item.command && !registry.has(item.command)).map(
      (item) => item.command,
    );
    expect([...new Set(missing)]).toEqual([]);
  });

  test("sort commands pass the column and direction", () => {
    const calls: string[] = [];
    const registry = setup();
    registry.run("sort.size", fakeContext(calls), { dir: "desc" });
    registry.run("sort.name", fakeContext(calls), { dir: "asc" });
    expect(calls).toEqual(["sort size desc", "sort name asc"]);
  });

  test("visual mode defaults to add", () => {
    const calls: string[] = [];
    const registry = setup();
    registry.run("selection.visual", fakeContext(calls), {});
    registry.run("selection.visual", fakeContext(calls), { mode: "remove" });
    expect(calls).toEqual(["visual add", "visual remove"]);
  });

  test("go to path opens the palette with the folder and a slash", () => {
    const calls: string[] = [];
    setup().run("palette.goToPath", fakeContext(calls), {});
    expect(calls).toEqual(["palette /Users/me/"]);
  });

  test("local-only commands are unavailable in remote locations", () => {
    const registry = setup();
    const remote = fakeContext([], { list: { remote: () => true } });
    expect(registry.available("go.home", remote)).toBe(false);
    expect(registry.available("go.downloads", remote)).toBe(false);
    expect(registry.available("search.content", remote)).toBe(false);
    expect(registry.available("go.overview", remote)).toBe(true);
  });

  test("list commands wait while the list is blocked", () => {
    const calls: string[] = [];
    const result = setup().run(
      "file.rename",
      fakeContext(calls, { list: { blocked: () => true } }),
      {},
    );
    expect(result).toBe(false);
    expect(calls).toEqual([]);
  });

  test("toggle keyboard mode and view commands reach the app", () => {
    const calls: string[] = [];
    const registry = setup();
    registry.run("app.toggleKeyboardMode", fakeContext(calls), {});
    registry.run("view.grid", fakeContext(calls), {});
    expect(calls).toEqual(["toggle mode", "view grid"]);
  });
});

describe("app.escape with visual mode", () => {
  test("leaves visual mode before anything else", () => {
    const calls: string[] = [];
    const context = fakeContext(calls, {
      checkboxes: { active: () => true, exit: () => void calls.push("exit checkboxes") },
      list: {
        exitVisual: () => {
          calls.push("exit visual");
          return true;
        },
      },
    });
    setup().run("app.escape", context, {});
    expect(calls).toEqual(["exit visual"]);
  });
});
