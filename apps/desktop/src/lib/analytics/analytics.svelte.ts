import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";

export type AnalyticsPrefs = {
  enabled: boolean;
  welcome_seen: boolean;
  install_id: string;
};

/** Reports are anonymous, only cover crashes/errors on the user's OS, and are locked
 *  on while the app is below 1.0. The Rust side enforces the same rule. */
class AnalyticsStore {
  enabled = $state(true);
  welcomeSeen = $state(true);
  version = $state("0.0.0");
  loaded = $state(false);

  /** True while the app version is below 1.0, when the toggle cannot be turned off. */
  get locked() {
    return Number(this.version.split(".")[0]) < 1;
  }

  async load() {
    try {
      const prefs = await invoke<AnalyticsPrefs>("analytics_prefs");
      this.enabled = prefs.enabled;
      this.welcomeSeen = prefs.welcome_seen;
    } catch (error) {
      console.warn("Couldn't read the error-reporting preference", error);
    }
    try {
      this.version = await getVersion();
    } catch (error) {
      console.warn("Couldn't read the app version", error);
    }
    if (this.locked) this.enabled = true;
    this.loaded = true;
  }

  async setEnabled(enabled: boolean) {
    await this.#save(this.locked ? true : enabled, this.welcomeSeen);
  }

  async completeWelcome(enabled: boolean) {
    await this.#save(this.locked ? true : enabled, true);
  }

  async #save(enabled: boolean, welcomeSeen: boolean) {
    this.enabled = enabled;
    this.welcomeSeen = welcomeSeen;
    try {
      const prefs = await invoke<AnalyticsPrefs>("save_analytics", { enabled, welcomeSeen });
      this.enabled = prefs.enabled;
      this.welcomeSeen = prefs.welcome_seen;
    } catch (error) {
      console.warn("Couldn't save the error-reporting preference", error);
    }
  }
}

export const analytics = new AnalyticsStore();
