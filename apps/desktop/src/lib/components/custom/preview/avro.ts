import { invoke } from "@tauri-apps/api/core";

export type AvroPreview = {
  schema: string;
  columns: string[];
  rows: string[][];
  truncated: boolean;
};

/** Reads the schema and first records of an `.avro` file. */
export function openAvro(path: string): Promise<AvroPreview> {
  return invoke<AvroPreview>("open_avro", { path });
}
