import { invoke } from "@tauri-apps/api/core";

export type ParquetPreview = {
  schema: string;
  columns: string[];
  rows: string[][];
  truncated: boolean;
};

/** Reads the schema and first records of a `.parquet` file. */
export function openParquet(path: string): Promise<ParquetPreview> {
  return invoke<ParquetPreview>("open_parquet", { path });
}
