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

export async function copyItems(
  paths: string[],
  destination: string,
  label: string,
  onStart?: (id: string) => void,
): Promise<DirectoryEntry[]> {
  const id = activity.start("copy", label, destination);
  onStart?.(id);
  try {
    return await invoke<DirectoryEntry[]>("copy_items", { paths, destination, jobId: id });
  } catch (error) {
    activity.fail(id, "copy", label, destination, error);
    throw error;
  }
}

export async function moveItems(
  paths: string[],
  destination: string,
  label: string,
  onStart?: (id: string) => void,
): Promise<DirectoryEntry[]> {
  const id = activity.start("move", label, destination);
  onStart?.(id);
  try {
    return await invoke<DirectoryEntry[]>("move_items", { paths, destination, jobId: id });
  } catch (error) {
    activity.fail(id, "move", label, destination, error);
    throw error;
  }
}

export async function deleteItems(paths: string[], label: string): Promise<void> {
  const id = activity.start("delete", label, "");
  try {
    await invoke<void>("delete_items", { paths, permanent: true, jobId: id });
  } catch (error) {
    activity.fail(id, "delete", label, "", error);
    throw error;
  }
}

export async function trashItems(paths: string[], label: string): Promise<void> {
  const id = activity.start("delete", label, "");
  try {
    await invoke<void>("delete_items", { paths, permanent: false, jobId: id });
  } catch (error) {
    activity.fail(id, "delete", label, "", error);
    throw error;
  }
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
