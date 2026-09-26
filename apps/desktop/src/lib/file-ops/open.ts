import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { activity } from "$lib/transfers/jobs.js";
import { baseName } from "./files.js";

export interface OpenWithApp {
  name: string;
  path: string;
  bundle_id: string;
  icon: string | null;
}

export interface TerminalInfo {
  id: string;
  name: string;
  icon: string | null;
}

export async function openTarget(path: string): Promise<void> {
  await activity.action(`Open: ${baseName(path)}`, path, () => invoke("open_path", { path }));
}

export function fetchOpenWithApps(path: string): Promise<OpenWithApp[]> {
  return invoke<OpenWithApp[]>("open_with_apps", { path });
}

/** The app the OS opens `path` with by default, or null when it can't tell. */
export function fetchDefaultApp(path: string): Promise<OpenWithApp | null> {
  return invoke<OpenWithApp | null>("default_app", { path });
}

export function openWithTarget(path: string, appPath: string): Promise<void> {
  return activity.action(`Open: ${baseName(path)}`, path, () =>
    invoke("open_with", { path, appPath }),
  );
}

/** Reveals `path` in the system file manager (Finder, File Explorer, …). */
export function revealPath(path: string): Promise<void> {
  return activity.action(`Reveal: ${baseName(path)}`, path, () => invoke("reveal_path", { path }));
}

/** Opens the operating system's terminal in the folder `path`. */
export function openTerminal(path: string, terminal: string, command = ""): Promise<void> {
  return activity.action("Open terminal", path, () =>
    invoke("open_terminal", { path, terminal, customCommand: command }),
  );
}

/** Terminals installed on this machine, for the settings dropdown. */
export function detectTerminals(): Promise<TerminalInfo[]> {
  return invoke<TerminalInfo[]>("detect_terminals");
}

/** Lets the user pick any application, like Finder's "Open With > Other…". */
export async function chooseApplication(): Promise<string | null> {
  const selected = await open({
    title: "Choose Application",
  });
  return typeof selected === "string" ? selected : null;
}
