export type KeyboardMode = "standard" | "yazi";
export type ChordTimeout = 1000 | 1500 | 3000;
/** Terminal id reported by the backend's detection ("system", "custom", or a detected app). */
export type TerminalApp = string;

export type Settings = {
  keyboardMode: KeyboardMode;
  showWhichKey: boolean;
  chordTimeoutMs: ChordTimeout;
  showHiddenFiles: boolean;
  defaultViewMode: "list" | "grid";
  previewOpenByDefault: boolean;
  panesLayout: "row" | "column";
  /** macOS trackpad swipe to go back and forward in the active pane. */
  swipeNavigation: boolean;
  /** Which terminal "Open Terminal Here" launches. */
  terminalApp: TerminalApp;
  /** Command template with a `{path}` placeholder, used when `terminalApp` is "custom". */
  terminalCommand: string;
  showFps: boolean;
  /** Development builds only. */
  prototypeSwitcher: boolean;
};

export type SettingKey = keyof Settings;
export type SettingsStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

export const SETTINGS_STORAGE_KEY = "settings.v1";
export const LEGACY_STORAGE_KEYS: readonly string[] = [
  "show-hidden-files",
  "show-fps",
  "dev-tools",
  "panes-layout",
];

export function defaultSettings({ dev }: { dev: boolean }): Settings {
  return {
    keyboardMode: "standard",
    showWhichKey: true,
    chordTimeoutMs: 1500,
    showHiddenFiles: false,
    defaultViewMode: "list",
    previewOpenByDefault: true,
    panesLayout: "row",
    swipeNavigation: true,
    terminalApp: "system",
    terminalCommand: "",
    showFps: dev,
    prototypeSwitcher: false,
  };
}

const isBoolean = (value: unknown): value is boolean => typeof value === "boolean";

const VALIDATORS: { [K in SettingKey]: (value: unknown) => value is Settings[K] } = {
  keyboardMode: (value): value is KeyboardMode => value === "standard" || value === "yazi",
  showWhichKey: isBoolean,
  chordTimeoutMs: (value): value is ChordTimeout =>
    value === 1000 || value === 1500 || value === 3000,
  showHiddenFiles: isBoolean,
  defaultViewMode: (value): value is "list" | "grid" => value === "list" || value === "grid",
  previewOpenByDefault: isBoolean,
  panesLayout: (value): value is "row" | "column" => value === "row" || value === "column",
  swipeNavigation: isBoolean,
  terminalApp: (value): value is TerminalApp => typeof value === "string",
  terminalCommand: (value): value is string => typeof value === "string",
  showFps: isBoolean,
  prototypeSwitcher: isBoolean,
};

export function isSettingKey(key: string): key is SettingKey {
  return Object.prototype.hasOwnProperty.call(VALIDATORS, key);
}

export function isValidSetting<K extends SettingKey>(key: K, value: unknown): value is Settings[K] {
  return VALIDATORS[key](value);
}

function assign<K extends SettingKey>(settings: Settings, key: K, value: Settings[K]) {
  settings[key] = value;
}

/** Settings saved as JSON. Invalid or missing fields fall back to their default. */
export function parseSettings(
  raw: string,
  defaults: Settings,
): { settings: Settings; problems: string[] } {
  let saved: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    saved =
      parsed && typeof parsed === "object" && !Array.isArray(parsed)
        ? (parsed as Record<string, unknown>)
        : {};
  } catch {
    return {
      settings: { ...defaults },
      problems: ["Saved settings are not valid JSON; using defaults"],
    };
  }
  const settings = { ...defaults };
  const problems: string[] = [];
  for (const key of Object.keys(defaults) as SettingKey[]) {
    if (!(key in saved)) continue;
    const value = saved[key];
    if (isValidSetting(key, value)) assign(settings, key, value);
    else problems.push(`Invalid setting “${key}”; using the default`);
  }
  return { settings, problems };
}

/** Preferences stored before settings existed. */
export function migrateLegacy(read: (key: string) => string | null, defaults: Settings): Settings {
  const settings = { ...defaults };
  const hidden = read("show-hidden-files");
  if (hidden !== null) settings.showHiddenFiles = hidden === "true";
  const fps = read("show-fps");
  if (fps !== null) settings.showFps = fps === "true";
  const devTools = read("dev-tools");
  if (devTools !== null) settings.prototypeSwitcher = devTools === "true";
  if (read("panes-layout") === "column") settings.panesLayout = "column";
  return settings;
}

export function loadSettings(
  storage: SettingsStorage,
  defaults: Settings,
): { settings: Settings; problems: string[] } {
  const raw = storage.getItem(SETTINGS_STORAGE_KEY);
  if (raw !== null) return parseSettings(raw, defaults);
  const settings = migrateLegacy((key) => storage.getItem(key), defaults);
  storage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  for (const key of LEGACY_STORAGE_KEYS) storage.removeItem(key);
  return { settings, problems: [] };
}
