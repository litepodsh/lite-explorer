import { invoke } from "@tauri-apps/api/core";
import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";

export function copyItem(path: string, destination: string): Promise<DirectoryEntry> {
  return invoke<DirectoryEntry>("copy_item", { path, destination });
}

export function moveItem(path: string, destination: string): Promise<DirectoryEntry> {
  return invoke<DirectoryEntry>("move_item", { path, destination });
}

export function trashItem(path: string): Promise<void> {
  return invoke("trash_item", { path });
}

export function deleteItem(path: string): Promise<void> {
  return invoke("delete_item", { path });
}

/** Folder containing `path`, with a trailing slash. Remote folder paths end in `/`
 *  themselves, so it is ignored when looking for the parent. */
export function parentPath(path: string): string {
  const trimmed = path.length > 1 && path.endsWith("/") ? path.slice(0, -1) : path;
  return trimmed.slice(0, trimmed.lastIndexOf("/") + 1);
}
