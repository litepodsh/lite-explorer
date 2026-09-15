import tauriConfig from "../../../desktop/src-tauri/tauri.conf.json";

/**
 * Read the app version from the desktop build so the site never drifts from
 * the shipped release. `tauri.conf.json` is the single source of truth.
 */
export const appVersion: string = (tauriConfig as { version?: string }).version ?? "0.0.0";
