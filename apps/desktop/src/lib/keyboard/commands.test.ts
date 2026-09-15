import { describe, expect, test } from "bun:test";
import { CommandRegistry, type Command } from "./commands.js";

type Context = { enabled: boolean; calls: string[] };

const command = (id: string, overrides: Partial<Command<Context>> = {}): Command<Context> => ({
  id,
  title: id,
  run: (context) => {
    context.calls.push(id);
  },
  ...overrides,
});

describe("CommandRegistry", () => {
  test("registers, lists and runs commands", () => {
    const registry = new CommandRegistry<Context>();
    registry.register([command("a"), command("b")]);
    const context: Context = { enabled: true, calls: [] };
    registry.run("b", context);
    expect(registry.list().map((item) => item.id)).toEqual(["a", "b"]);
    expect(registry.has("a")).toBe(true);
    expect(context.calls).toEqual(["b"]);
  });

  test("rejects duplicate ids", () => {
    const registry = new CommandRegistry<Context>();
    registry.register([command("a")]);
    let message = "";
    try {
      registry.register([command("a")]);
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toBe("Command “a” is already registered");
  });

  test("unregister removes only its own commands", () => {
    const registry = new CommandRegistry<Context>();
    const stop = registry.register([command("a")]);
    registry.register([command("b")]);
    stop();
    expect(registry.has("a")).toBe(false);
    expect(registry.has("b")).toBe(true);
  });

  test("available follows when and unknown ids", () => {
    const registry = new CommandRegistry<Context>();
    registry.register([command("a", { when: (context) => context.enabled })]);
    expect(registry.available("a", { enabled: true, calls: [] })).toBe(true);
    expect(registry.available("a", { enabled: false, calls: [] })).toBe(false);
    expect(registry.available("missing", { enabled: true, calls: [] })).toBe(false);
  });

  test("run skips unavailable commands and returns false", () => {
    const registry = new CommandRegistry<Context>();
    registry.register([command("a", { when: () => false })]);
    const context: Context = { enabled: true, calls: [] };
    expect(registry.run("a", context)).toBe(false);
    expect(context.calls).toEqual([]);
  });
});
