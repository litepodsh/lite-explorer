import { describe, expect, test } from "bun:test";
import {
  acceleratorToAuthored,
  codeLabel,
  eventToken,
  formatToken,
  normalizeToken,
  toKeyPlatform,
  type KeyInput,
} from "./keys.js";

const press = (
  key: string,
  code: string,
  mods: Partial<{
    ctrl: boolean;
    alt: boolean;
    shift: boolean;
    meta: boolean;
    composing: boolean;
  }> = {},
): KeyInput => ({
  key,
  code,
  ctrlKey: mods.ctrl ?? false,
  altKey: mods.alt ?? false,
  shiftKey: mods.shift ?? false,
  metaKey: mods.meta ?? false,
  isComposing: mods.composing ?? false,
});

describe("codeLabel", () => {
  test("labels letters, digits, function keys and named codes", () => {
    expect(codeLabel("KeyP")).toBe("P");
    expect(codeLabel("Digit7")).toBe("7");
    expect(codeLabel("F6")).toBe("<F6>");
    expect(codeLabel("Period")).toBe(".");
    expect(codeLabel("ArrowLeft")).toBe("<Left>");
    expect(codeLabel("IntlBackslash")).toBeNull();
  });
});

describe("eventToken", () => {
  test("printable keys without command modifiers are the character", () => {
    expect(eventToken(press("j", "KeyJ"))).toBe("j");
    expect(eventToken(press("G", "KeyG", { shift: true }))).toBe("G");
    expect(eventToken(press("?", "Slash", { shift: true }))).toBe("?");
  });

  test("named keys keep Shift", () => {
    expect(eventToken(press("ArrowDown", "ArrowDown"))).toBe("<Down>");
    expect(eventToken(press("ArrowDown", "ArrowDown", { shift: true }))).toBe("Shift+<Down>");
    expect(eventToken(press(" ", "Space"))).toBe("<Space>");
    expect(eventToken(press("Escape", "Escape"))).toBe("<Esc>");
    expect(eventToken(press("F6", "F6", { shift: true }))).toBe("Shift+<F6>");
  });

  test("command modifiers use the physical key", () => {
    expect(eventToken(press("p", "KeyP", { meta: true, shift: true }))).toBe("Shift+Meta+P");
    expect(eventToken(press("π", "KeyP", { alt: true }))).toBe("Alt+P");
    expect(eventToken(press("&", "Digit1", { ctrl: true }))).toBe("Ctrl+1");
    expect(eventToken(press("Backspace", "Backspace", { meta: true, shift: true }))).toBe(
      "Shift+Meta+<Backspace>",
    );
  });

  test("non-Latin letters fall back to the physical key", () => {
    expect(eventToken(press("о", "KeyJ"))).toBe("j");
    expect(eventToken(press("О", "KeyJ", { shift: true }))).toBe("J");
  });

  test("ignores composition, dead keys, lone modifiers and AltGr text", () => {
    expect(eventToken(press("j", "KeyJ", { composing: true }))).toBeNull();
    expect(eventToken(press("Dead", "BracketRight"))).toBeNull();
    expect(eventToken(press("Shift", "ShiftLeft", { shift: true }))).toBeNull();
    expect(eventToken(press("@", "KeyQ", { ctrl: true, alt: true }))).toBeNull();
  });
});

describe("normalizeToken", () => {
  test("resolves Mod per platform and orders modifiers", () => {
    expect(normalizeToken("Mod+Shift+P", "macos")).toBe("Shift+Meta+P");
    expect(normalizeToken("Mod+Shift+P", "windows")).toBe("Ctrl+Shift+P");
    expect(normalizeToken("Shift+Mod+<Left>", "linux")).toBe("Ctrl+Shift+<Left>");
    expect(normalizeToken("Ctrl+d", "macos")).toBe("Ctrl+D");
  });

  test("leaves plain keys untouched", () => {
    expect(normalizeToken("g", "macos")).toBe("g");
    expect(normalizeToken("<Space>", "windows")).toBe("<Space>");
    expect(normalizeToken("Shift+<F6>", "macos")).toBe("Shift+<F6>");
  });
});

describe("acceleratorToAuthored", () => {
  test("converts Tauri accelerators", () => {
    expect(acceleratorToAuthored("CmdOrCtrl+Shift+Period")).toBe("Mod+Shift+.");
    expect(acceleratorToAuthored("CmdOrCtrl+Shift+]")).toBe("Mod+Shift+]");
    expect(acceleratorToAuthored("CmdOrCtrl+Shift+Alt+N")).toBe("Mod+Shift+Alt+N");
    expect(acceleratorToAuthored("CmdOrCtrl+Comma")).toBe("Mod+,");
    expect(acceleratorToAuthored("Hyper+K")).toBeNull();
  });
});

describe("formatToken", () => {
  test("uses symbols on macOS", () => {
    expect(formatToken("Shift+Meta+P", "macos")).toBe("⇧⌘P");
    expect(formatToken("Ctrl+D", "macos")).toBe("⌃D");
    expect(formatToken("<Down>", "macos")).toBe("↓");
    expect(formatToken("Meta+<Backspace>", "macos")).toBe("⌘⌫");
  });

  test("uses words elsewhere", () => {
    expect(formatToken("Ctrl+Shift+P", "windows")).toBe("Ctrl+Shift+P");
    expect(formatToken("Shift+<Delete>", "linux")).toBe("Shift+Del");
    expect(formatToken("<F1>", "windows")).toBe("F1");
    expect(formatToken("?", "windows")).toBe("?");
  });
});

describe("toKeyPlatform", () => {
  test("treats unknown platforms like Windows", () => {
    expect(toKeyPlatform("macos")).toBe("macos");
    expect(toKeyPlatform("linux")).toBe("linux");
    expect(toKeyPlatform("unknown")).toBe("windows");
  });
});
