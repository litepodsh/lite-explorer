import { activity } from "$lib/transfers/jobs.js";
import { invoke } from "@tauri-apps/api/core";
import type { DirectoryEntry } from "$lib/components/custom/file-list/list-item.svelte";

export type CreateKind = "folder" | "file";

export interface CreateItemAction {
  readonly kind: CreateKind;
  readonly label: string;
  defaultName(): string;
  create(parentPath: string, name: string): Promise<DirectoryEntry>;
}

export const createFolderAction: CreateItemAction = {
  kind: "folder",
  label: "New Folder",
  defaultName: () => "untitled folder",
  create: (parentPath, name) =>
    activity.track("create", `Create folder: ${name}`, parentPath, () => invoke<DirectoryEntry>("create_item", { parent: parentPath, kind: "folder", name })),
};

export const createFileAction: CreateItemAction = {
  kind: "file",
  label: "New File",
  defaultName: () => "untitled.txt",
  create: (parentPath, name) =>
    activity.track("create", `Create file: ${name}`, parentPath, () => invoke<DirectoryEntry>("create_item", { parent: parentPath, kind: "file", name })),
};

export const createItemActions: Record<CreateKind, CreateItemAction> = {
  folder: createFolderAction,
  file: createFileAction,
};

export function nextDefaultName(kind: CreateKind): string {
  return createItemActions[kind].defaultName();
}
