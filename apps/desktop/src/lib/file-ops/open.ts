import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

export interface OpenWithApp {
  name: string;
  path: string;
  bundle_id: string;
  icon: string | null;
}

export async function openTarget(path: string): Promise<void> {
  await invoke("open_path", { path });
}

export function fetchOpenWithApps(path: string): Promise<OpenWithApp[]> {
  return invoke<OpenWithApp[]>("open_with_apps", { path });
}

/** The app the OS opens `path` with by default, or null when it can't tell. */
export function fetchDefaultApp(path: string): Promise<OpenWithApp | null> {
  return invoke<OpenWithApp | null>("default_app", { path });
}

export function openWithTarget(path: string, appPath: string): Promise<void> {
  return invoke("open_with", { path, appPath });
}

/** Reveals `path` in the system file manager (Finder, File Explorer, …). */
export function revealPath(path: string): Promise<void> {
  return invoke("reveal_path", { path });
}

/** Lets the user pick any application, like Finder's "Open With > Other…". */
export async function chooseApplication(): Promise<string | null> {
  const selected = await open({
    title: "Choose Application",
    defaultPath: "/Applications",
    filters: [{ name: "Applications", extensions: ["app"] }],
  });
  return typeof selected === "string" ? selected : null;
}
