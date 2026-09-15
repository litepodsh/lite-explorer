import { dev } from "$app/environment";
import { emit, listen } from "@tauri-apps/api/event";
import {
  SETTINGS_STORAGE_KEY,
  defaultSettings,
  isSettingKey,
  isValidSetting,
  loadSettings,
  type SettingKey,
  type Settings,
} from "./settings.js";

export const SETTINGS_CHANGED = "settings-changed";

/** Preferences shared by every window. Changes persist and reach the other windows. */
class SettingsStore {
  current = $state<Settings>(defaultSettings({ dev }));
  #loaded = false;

  /** Reads saved settings. Call at the top of each window's page, before creating stores. */
  load() {
    if (this.#loaded) return;
    this.#loaded = true;
    const { settings, problems } = loadSettings(localStorage, defaultSettings({ dev }));
    for (const problem of problems) console.warn(problem);
    this.current = settings;
  }

  set<K extends SettingKey>(key: K, value: Settings[K]) {
    if (this.current[key] === value) return;
    this.current[key] = value;
    localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify($state.snapshot(this.current)));
    void emit(SETTINGS_CHANGED, { key, value }).catch((error: unknown) =>
      console.warn("Couldn't share a settings change with other windows", error),
    );
  }

  reset() {
    const defaults = defaultSettings({ dev });
    for (const key of Object.keys(defaults) as SettingKey[]) this.#setDefault(key, defaults);
  }

  /** Applies changes made in other windows. Returns the function that stops listening. */
  listen(): () => void {
    const unlisten = listen<{ key: string; value: unknown }>(SETTINGS_CHANGED, ({ payload }) => {
      if (isSettingKey(payload.key)) this.#apply(payload.key, payload.value);
    });
    return () => void unlisten.then((stop) => stop());
  }

  #setDefault<K extends SettingKey>(key: K, defaults: Settings) {
    this.set(key, defaults[key]);
  }

  #apply<K extends SettingKey>(key: K, value: unknown) {
    if (isValidSetting(key, value) && this.current[key] !== value) this.current[key] = value;
  }
}

export const settings = new SettingsStore();
