import { invoke } from "@tauri-apps/api/core";
import type { DatabasePreview, TableData } from "./database";

export type { DatabasePreview, TableData };

/** Lists the tables of a DuckDB database. */
export function openDuckdb(path: string): Promise<DatabasePreview> {
  return invoke<DatabasePreview>("open_duckdb", { path });
}

/** Reads a bounded grid for one DuckDB table. */
export function readDuckdbTable(path: string, table: string): Promise<TableData> {
  return invoke<TableData>("read_duckdb_table", { path, table });
}
