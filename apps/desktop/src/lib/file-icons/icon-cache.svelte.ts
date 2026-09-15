import { invoke } from "@tauri-apps/api/core";
import { SvelteMap } from "svelte/reactivity";

/** Native icon for a path: a PNG data URL, or `null` when the OS had none. */
type CachedIcon = string | null;

const icons = new SvelteMap<string, CachedIcon>();
const pending = new Set<string>();
let scheduled = false;

/**
 * The list is virtualized, so only mounted rows call `requestIcon`. Every
 * request in the same tick is collected and sent as one `file_icons` call,
 * which keeps the IPC count proportional to scrolling, not to folder size.
 */
function flush(): void {
  scheduled = false;
  if (pending.size === 0) return;
  const batch = [...pending];
  pending.clear();
  void invoke<CachedIcon[]>("file_icons", { paths: batch })
    .then((results) => {
      for (let index = 0; index < batch.length; index++) {
        icons.set(batch[index], results[index] ?? null);
      }
    })
    .catch(() => {
      // A failed batch must not retry forever; cache the misses.
      for (const path of batch) icons.set(path, null);
    });
}

/** Ask for a path's icon once. Already cached or queued paths are ignored. */
export function requestIcon(path: string): void {
  if (icons.has(path) || pending.has(path)) return;
  pending.add(path);
  if (!scheduled) {
    scheduled = true;
    queueMicrotask(flush);
  }
}

/** Reactive read: re-renders the caller when the icon for `path` arrives. */
export function iconFor(path: string): CachedIcon | undefined {
  return icons.get(path);
}

/** Test-only: clears the cache and any queued requests. */
export function resetIconCache(): void {
  icons.clear();
  pending.clear();
}
