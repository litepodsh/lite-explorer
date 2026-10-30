import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import { SvelteMap } from "svelte/reactivity";

const thumbnails = new SvelteMap<string, string | null>();
const pending = new Set<string>();
const inFlight = new Set<string>();
const checkingCache = new Set<string>();
let scheduled = false;
let activeWorkers = 0;
let currentGeneration = 1;
let activeFolderKey = "";

const MAX_MEMORY_ENTRIES = 2048;
const BATCH_SIZE = 6;
const MAX_CONCURRENT_BATCHES = 2;

function remember(path: string, thumbnail: string | null) {
  thumbnails.delete(path);
  thumbnails.set(path, thumbnail);
  if (thumbnails.size > MAX_MEMORY_ENTRIES) {
    thumbnails.delete(thumbnails.keys().next().value!);
  }
}

function pump() {
  scheduled = false;
  while (activeWorkers < MAX_CONCURRENT_BATCHES && pending.size > 0) {
    // Take the most recently requested (currently in-viewport) items first.
    const all = [...pending];
    const batch = all.slice(-BATCH_SIZE);
    for (const path of batch) {
      pending.delete(path);
      inFlight.add(path);
    }
    const batchGeneration = currentGeneration;
    activeWorkers++;
    void invoke<{ path: string; url: string }[]>("image_thumbnails", {
      paths: batch,
      generation: batchGeneration,
      cachedOnly: false,
    })
      .then((items) => {
        const found = new Map(items.map((item) => [item.path, item.url]));
        const superseded = batchGeneration < currentGeneration;
        for (const path of batch) {
          inFlight.delete(path);
          const url = found.get(path);
          if (url !== undefined) {
            remember(path, url);
          } else if (!superseded) {
            // Only cache a negative result when the batch completed normally
            // without being cancelled by a folder switch.
            remember(path, null);
          }
        }
      })
      .catch(() => {
        const superseded = batchGeneration < currentGeneration;
        for (const path of batch) {
          inFlight.delete(path);
          if (!superseded) remember(path, null);
        }
      })
      .finally(() => {
        if (batchGeneration === currentGeneration) {
          activeWorkers = Math.max(0, activeWorkers - 1);
        }
        if (pending.size > 0) pump();
      });
  }
}

/**
 * Cancels pending and in-flight thumbnail generation from a previous folder.
 */
export function cancelPendingThumbnails() {
  currentGeneration++;
  activeWorkers = 0;
  pending.clear();
  inFlight.clear();
  void invoke("cancel_thumbnails", { generation: currentGeneration }).catch(() => {});
}

/**
 * Requests thumbnails for paths currently visible in the viewport.
 * If `viewportOnly` is true (default for file lists), queued paths that scrolled
 * out of the viewport before being dispatched are dropped so visible items load immediately.
 * If `folderKey` changes, any pending/queued generation from the previous folder is cancelled
 * immediately, while disk-cached thumbnails for the new view resolve right away without waiting
 * on heavy decodes.
 */
export function requestThumbnails(
  paths: readonly string[],
  viewportOnly = true,
  folderKey?: string,
) {
  untrack(() => {
    if (folderKey !== undefined && folderKey !== activeFolderKey) {
      activeFolderKey = folderKey;
      cancelPendingThumbnails();
    }

    if (viewportOnly) {
      const visible = new Set(paths);
      for (const queued of pending) {
        if (!visible.has(queued)) pending.delete(queued);
      }
    }

    const uncached: string[] = [];
    for (const path of paths) {
      if (thumbnails.has(path)) {
        continue;
      }
      if (!inFlight.has(path) && !checkingCache.has(path)) {
        uncached.push(path);
      }
    }

    if (uncached.length === 0) return;

    // Fast-path disk cache lookup: resolve any already-generated thumbnails on disk
    // immediately without waiting in the heavy decode queue, then queue only cache misses.
    const lookupGeneration = currentGeneration;
    for (const path of uncached) checkingCache.add(path);

    void invoke<{ path: string; url: string }[]>("image_thumbnails", {
      paths: uncached,
      generation: lookupGeneration,
      cachedOnly: true,
    })
      .then((cachedItems) => {
        const found = new Map(cachedItems.map((item) => [item.path, item.url]));
        for (const [path, url] of found) {
          checkingCache.delete(path);
          pending.delete(path);
          remember(path, url);
        }
        const superseded = lookupGeneration < currentGeneration;
        for (const path of uncached) {
          checkingCache.delete(path);
          if (found.has(path) || superseded) continue;
          if (!thumbnails.has(path) && !inFlight.has(path)) {
            pending.delete(path);
            pending.add(path);
          }
        }
        if (pending.size > 0 && !scheduled) {
          scheduled = true;
          queueMicrotask(pump);
        }
      })
      .catch(() => {
        for (const path of uncached) checkingCache.delete(path);
      });
  });
}

export function thumbnailFor(path: string) {
  return thumbnails.get(path);
}

/** Test-only: clears the cache and any queued requests. */
export function resetThumbnailCache() {
  thumbnails.clear();
  pending.clear();
  inFlight.clear();
  checkingCache.clear();
  scheduled = false;
  activeWorkers = 0;
  currentGeneration = 1;
  activeFolderKey = "";
}
