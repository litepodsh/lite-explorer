import { settings } from "$lib/settings/settings.svelte.js";
import { openTerminal } from "$lib/file-ops/open.js";

/** Opens the configured terminal in `path`. */
export function openTerminalHere(path: string): Promise<void> {
  return openTerminal(path, settings.current.terminalApp, settings.current.terminalCommand);
}
