import { describe, expect, test } from "bun:test";
import { formatAccelerator, matchesAccelerator, parseAccelerator } from "./accelerator.js";

const key = (
  code: string,
  mods: Partial<{ ctrl: boolean; meta: boolean; alt: boolean; shift: boolean }> = {},
) => ({
  code,
  ctrlKey: mods.ctrl ?? false,
  metaKey: mods.meta ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
});

describe("parseAccelerator", () => {
  test("CmdOrCtrl is Ctrl on Windows and Linux, Meta on macOS", () => {
    expect(parseAccelerator("CmdOrCtrl+T", "windows")).toEqual({
      ctrl: true,
      meta: false,
      alt: false,
      shift: false,
      code: "KeyT",
    });
    expect(parseAccelerator("CmdOrCtrl+T", "linux")?.ctrl).toBe(true);
    expect(parseAccelerator("CmdOrCtrl+T", "macos")).toEqual({
      ctrl: false,
      meta: true,
      alt: false,
      shift: false,
      code: "KeyT",
    });
  });

  test("named and symbol keys map to KeyboardEvent.code", () => {
    expect(parseAccelerator("CmdOrCtrl+Shift+Period", "windows")?.code).toBe("Period");
    expect(parseAccelerator("CmdOrCtrl+Shift+]", "windows")?.code).toBe("BracketRight");
    expect(parseAccelerator("CmdOrCtrl+Shift+[", "windows")?.code).toBe("BracketLeft");
    expect(parseAccelerator("Ctrl+1", "linux")?.code).toBe("Digit1");
    expect(parseAccelerator("F5", "linux")?.code).toBe("F5");
  });

  test("all modifiers", () => {
    expect(parseAccelerator("CmdOrCtrl+Shift+Alt+N", "windows")).toEqual({
      ctrl: true,
      meta: false,
      alt: true,
      shift: true,
      code: "KeyN",
    });
  });

  test("unknown parts return null", () => {
    expect(parseAccelerator("Hyper+T", "windows")).toBeNull();
    expect(parseAccelerator("CmdOrCtrl+Pause", "windows")).toBeNull();
    expect(parseAccelerator("", "windows")).toBeNull();
  });
});

describe("formatAccelerator", () => {
  test("formats in Ctrl, Shift, Alt order with readable keys", () => {
    expect(formatAccelerator(parseAccelerator("CmdOrCtrl+Shift+Period", "windows")!)).toBe(
      "Ctrl+Shift+.",
    );
    expect(formatAccelerator(parseAccelerator("CmdOrCtrl+Shift+Alt+N", "windows")!)).toBe(
      "Ctrl+Shift+Alt+N",
    );
    expect(formatAccelerator(parseAccelerator("CmdOrCtrl+Shift+]", "linux")!)).toBe("Ctrl+Shift+]");
  });
});

describe("matchesAccelerator", () => {
  const newTab = parseAccelerator("CmdOrCtrl+T", "windows")!;

  test("matches exact modifiers and code", () => {
    expect(matchesAccelerator(key("KeyT", { ctrl: true }), newTab)).toBe(true);
  });

  test("extra or missing modifiers do not match", () => {
    expect(matchesAccelerator(key("KeyT", { ctrl: true, shift: true }), newTab)).toBe(false);
    expect(matchesAccelerator(key("KeyT"), newTab)).toBe(false);
    expect(matchesAccelerator(key("KeyT", { meta: true }), newTab)).toBe(false);
  });
});
