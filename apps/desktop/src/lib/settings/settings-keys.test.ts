import { describe, expect, test } from "bun:test";
import { settingsKeyAction } from "./settings-keys.js";

const options = {
  platform: "macos" as const,
  mode: "standard" as const,
  inNav: false,
  sectionCount: 3,
};

describe("settingsKeyAction", () => {
  test("Escape and Mod+W close the window", () => {
    expect(settingsKeyAction("<Esc>", options)).toEqual({ kind: "close" });
    expect(settingsKeyAction("Meta+W", options)).toEqual({ kind: "close" });
    expect(settingsKeyAction("Ctrl+W", { ...options, platform: "windows" })).toEqual({
      kind: "close",
    });
    expect(settingsKeyAction("Ctrl+W", options)).toBeNull();
  });

  test("Mod+digit jumps to a section that exists", () => {
    expect(settingsKeyAction("Meta+2", options)).toEqual({ kind: "section", index: 1 });
    expect(settingsKeyAction("Meta+4", options)).toBeNull();
  });

  test("arrows move only inside the section list", () => {
    expect(settingsKeyAction("<Down>", options)).toBeNull();
    expect(settingsKeyAction("<Down>", { ...options, inNav: true })).toEqual({
      kind: "move",
      delta: 1,
    });
    expect(settingsKeyAction("<Up>", { ...options, inNav: true })).toEqual({
      kind: "move",
      delta: -1,
    });
  });

  test("j and k move only in Yazi mode", () => {
    expect(settingsKeyAction("j", { ...options, inNav: true })).toBeNull();
    expect(settingsKeyAction("j", { ...options, inNav: true, mode: "yazi" })).toEqual({
      kind: "move",
      delta: 1,
    });
    expect(settingsKeyAction("k", { ...options, inNav: true, mode: "yazi" })).toEqual({
      kind: "move",
      delta: -1,
    });
  });
});
