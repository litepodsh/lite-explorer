import { invoke } from "@tauri-apps/api/core";

export type IsoEntry = {
  path: string;
  size: number;
  isDir: boolean;
};

export type IsoPreview = {
  volumeName: string;
  entries: IsoEntry[];
  truncated: boolean;
};

/** Lists the files in an `.iso` disc image. */
export function openIso(path: string): Promise<IsoPreview> {
  return invoke<IsoPreview>("open_iso", { path });
}
