import { describe, expect, test } from "bun:test";
import {
  LEGACY_STORAGE_KEYS,
  SETTINGS_STORAGE_KEY,
  defaultSettings,
  isSettingKey,
  isValidSetting,
  loadSettings,
  migrateLegacy,
  parseSettings,
  type SettingsStorage,
} from "./settings.js";

function memoryStorage(
  initial: Record<string, string> = {},
): SettingsStorage & { data: Map<string, string> } {
  const data = new Map(Object.entries(initial));
  return {
    data,
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => void data.set(key, value),
    removeItem: (key) => void data.delete(key),
  };
}

describe("defaultSettings", () => {
  test("FPS meter defaults to on only in development", () => {
    expect(defaultSettings({ dev: true }).showFps).toBe(true);
    expect(defaultSettings({ dev: false }).showFps).toBe(false);
    expect(defaultSettings({ dev: false }).keyboardMode).toBe("standard");
    expect(defaultSettings({ dev: false }).chordTimeoutMs).toBe(1500);
    expect(defaultSettings({ dev: false }).terminalApp).toBe("system");
    expect(defaultSettings({ dev: false }).terminalCommand).toBe("");
    expect(defaultSettings({ dev: false }).previewOpenByDefault).toBe(false);
  });
});

describe("validation", () => {
  test("knows setting keys", () => {
    expect(isSettingKey("keyboardMode")).toBe(true);
    expect(isSettingKey("toString")).toBe(false);
  });

  test("checks values per key", () => {
    expect(isValidSetting("keyboardMode", "yazi")).toBe(true);
    expect(isValidSetting("keyboardMode", "vim")).toBe(false);
    expect(isValidSetting("chordTimeoutMs", 3000)).toBe(true);
    expect(isValidSetting("chordTimeoutMs", 2000)).toBe(false);
    expect(isValidSetting("showFps", "true")).toBe(false);
    expect(isValidSetting("terminalApp", "ghostty")).toBe(true);
    expect(isValidSetting("terminalApp", "my-term")).toBe(true);
    expect(isValidSetting("terminalApp", 42)).toBe(false);
    expect(isValidSetting("terminalCommand", "kitty --directory {path}")).toBe(true);
    expect(isValidSetting("terminalCommand", 42)).toBe(false);
  });
});

describe("parseSettings", () => {
  const defaults = defaultSettings({ dev: false });

  test("keeps valid fields and replaces invalid ones", () => {
    const raw = JSON.stringify({ keyboardMode: "yazi", panesLayout: "diagonal", unknown: 1 });
    const { settings, problems } = parseSettings(raw, defaults);
    expect(settings.keyboardMode).toBe("yazi");
    expect(settings.panesLayout).toBe("row");
    expect(problems).toEqual(["Invalid setting “panesLayout”; using the default"]);
  });

  test("corrupt JSON falls back to defaults", () => {
    const { settings, problems } = parseSettings("{not json", defaults);
    expect(settings).toEqual(defaults);
    expect(problems).toEqual(["Saved settings are not valid JSON; using defaults"]);
  });
});

describe("migrateLegacy", () => {
  test("reads the old keys", () => {
    const old: Record<string, string> = {
      "show-hidden-files": "true",
      "show-fps": "false",
      "dev-tools": "true",
      "panes-layout": "column",
    };
    const settings = migrateLegacy((key) => old[key] ?? null, defaultSettings({ dev: true }));
    expect(settings.showHiddenFiles).toBe(true);
    expect(settings.showFps).toBe(false);
    expect(settings.prototypeSwitcher).toBe(true);
    expect(settings.panesLayout).toBe("column");
  });

  test("missing old keys keep defaults", () => {
    const settings = migrateLegacy(() => null, defaultSettings({ dev: true }));
    expect(settings).toEqual(defaultSettings({ dev: true }));
  });
});

describe("loadSettings", () => {
  test("migrates once, saves and removes the old keys", () => {
    const storage = memoryStorage({ "show-hidden-files": "true", "sidebar-width": "240" });
    const { settings } = loadSettings(storage, defaultSettings({ dev: false }));
    expect(settings.showHiddenFiles).toBe(true);
    expect(JSON.parse(storage.data.get(SETTINGS_STORAGE_KEY) ?? "{}").showHiddenFiles).toBe(true);
    expect(LEGACY_STORAGE_KEYS.filter((key) => storage.data.has(key))).toEqual([]);
    expect(storage.data.get("sidebar-width")).toBe("240");
  });

  test("reads saved settings without migrating", () => {
    const storage = memoryStorage({
      [SETTINGS_STORAGE_KEY]: JSON.stringify({ keyboardMode: "yazi" }),
      "show-hidden-files": "true",
    });
    const { settings } = loadSettings(storage, defaultSettings({ dev: false }));
    expect(settings.keyboardMode).toBe("yazi");
    expect(settings.showHiddenFiles).toBe(false);
  });
});

test("automatic size preferences persist and old settings keep them disabled", () => {
  const defaults = defaultSettings({ dev: false });
  expect(parseSettings("{}", defaults).settings.automaticSizesInHome).toBe(false);
  expect(parseSettings("{}", defaults).settings.automaticSizePaths).toEqual([]);
  const storage = memoryStorage({
    [SETTINGS_STORAGE_KEY]: JSON.stringify({ automaticSizesInHome: true, automaticSizePaths: ["/data/work"] }),
  });
  const { settings, problems } = loadSettings(storage, defaults);
  expect(settings.automaticSizesInHome).toBe(true);
  expect(settings.automaticSizePaths).toEqual(["/data/work"]);
  expect(problems).toEqual([]);
  expect(isValidSetting("automaticSizePaths", [42])).toBe(false);
  expect(isValidSetting("automaticSizePaths", [""])).toBe(false);
  expect(isValidSetting("automaticSizePaths", "/data")).toBe(false);
  expect(isValidSetting("automaticSizesInHome", "true")).toBe(false);
});
