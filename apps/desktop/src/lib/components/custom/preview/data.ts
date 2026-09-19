import { invoke } from "@tauri-apps/api/core";

export type DataPreview = {
  format: string;
  value: unknown;
  truncated: boolean;
};

/** Parses a `.json`/`.ndjson`/`.yaml`/`.toml` file into a foldable tree. */
export function openData(path: string): Promise<DataPreview> {
  return invoke<DataPreview>("open_data", { path });
}
