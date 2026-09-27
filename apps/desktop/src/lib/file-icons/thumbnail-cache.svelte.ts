import { invoke } from "@tauri-apps/api/core";
import { SvelteMap } from "svelte/reactivity";

const thumbnails = new SvelteMap<string, string | null>();
const pending = new Set<string>();
let scheduled = false;

function flush() {
  scheduled = false;
  const paths = [...pending];
  pending.clear();
  if (!paths.length) return;
  void invoke<{ path: string; url: string }[]>("image_thumbnails", { paths })
    .then((items) => {
      const found = new Map(items.map((item) => [item.path, item.url]));
      for (const path of paths) thumbnails.set(path, found.get(path) ?? null);
    })
    .catch(() => paths.forEach((path) => thumbnails.set(path, null)));
}

export function requestThumbnails(paths: readonly string[]) {
  for (const path of paths) if (!thumbnails.has(path) && !pending.has(path)) pending.add(path);
  if (pending.size && !scheduled) { scheduled = true; queueMicrotask(flush); }
}

export function thumbnailFor(path: string) { return thumbnails.get(path); }
