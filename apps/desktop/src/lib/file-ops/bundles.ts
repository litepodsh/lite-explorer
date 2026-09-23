import { canDragOut } from "$lib/file-drag/native-drag.js";
import type { Platform } from "$lib/state/platform.svelte.js";

/**
 * macOS app bundles are directories that Finder treats as a single item: they
 * show the app's icon and launch on open. Their contents are reached through
 * "Show Package Contents".
 */
export function isAppBundle(
  entry: { path: string; name: string; is_directory: boolean; kind?: string; inner_path?: string | null },
  platform: Platform,
): boolean {
  return (
    platform === "macos" &&
    entry.is_directory &&
    entry.inner_path == null &&
    entry.name.toLowerCase().endsWith(".app") &&
    canDragOut(entry)
  );
}
