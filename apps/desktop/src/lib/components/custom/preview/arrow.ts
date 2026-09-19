import { invoke } from "@tauri-apps/api/core";

export type ArrowPreview = {
  schema: string;
  columns: string[];
  rows: string[][];
  truncated: boolean;
};

/** Reads the schema and first records of an Arrow `.arrow`/`.feather` file. */
export function openArrow(path: string): Promise<ArrowPreview> {
  return invoke<ArrowPreview>("open_arrow", { path });
}
