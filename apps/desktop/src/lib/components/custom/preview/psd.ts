import { invoke } from "@tauri-apps/api/core";

export type PsdLayer = {
  name: string;
  kind: string;
  width: number;
  height: number;
  visible: boolean;
};

export type PsdPreview = {
  width: number;
  height: number;
  colorMode: string;
  layers: PsdLayer[];
  truncated: boolean;
};

/** Reads document metadata and the layer stack of a `.psd` file. */
export function openPsd(path: string): Promise<PsdPreview> {
  return invoke<PsdPreview>("open_psd", { path });
}
