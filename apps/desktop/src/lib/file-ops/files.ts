import { activity } from "$lib/transfers/jobs.js";
import { invoke } from "@tauri-apps/api/core";
import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";

export function copyItem(path: string, destination: string): Promise<DirectoryEntry> {
  return activity.track("copy", `Copy: ${baseName(path)}`, destination, () =>
    invoke<DirectoryEntry>("copy_item", { path, destination }),
  );
}

export function moveItem(path: string, destination: string): Promise<DirectoryEntry> {
  return activity.track("move", `Move: ${baseName(path)}`, destination, () =>
    invoke<DirectoryEntry>("move_item", { path, destination }),
  );
}

export function trashItem(path: string): Promise<void> {
  return activity.track("delete", `Move to Trash: ${baseName(path)}`, "", () =>
    invoke<void>("trash_item", { path }),
  );
}

export function deleteItem(path: string): Promise<void> {
  return activity.track("delete", `Delete: ${baseName(path)}`, "", () =>
    invoke<void>("delete_item", { path }),
  );
}

const isSeparator = (character: string) => character === "/" || character === "\\";

/** Folder containing `path`, with a trailing separator. Remote folder paths end in `/`
 *  themselves, so it is ignored when looking for the parent. Windows paths use `\`. */
export function parentPath(path: string): string {
  const trimmed = path.length > 1 && isSeparator(path.at(-1)!) ? path.slice(0, -1) : path;
  return trimmed.slice(0, Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\")) + 1);
}

/** Last segment of a local, remote or Windows path, ignoring a trailing separator. */
export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? "";
}
