import { invoke } from "@tauri-apps/api/core";

export type TableInfo = {
  name: string;
  rows: number;
};

export type ColumnInfo = {
  name: string;
  type: string;
  primaryKey: boolean;
  notNull: boolean;
};

export type TableData = {
  name: string;
  columns: ColumnInfo[];
  rows: string[][];
  truncated: boolean;
};

export type DatabasePreview = {
  tables: TableInfo[];
};

/** Lists the tables of a SQLite database. */
export function openDatabase(path: string): Promise<DatabasePreview> {
  return invoke<DatabasePreview>("open_database", { path });
}

/** Reads a bounded grid for one SQLite table. */
export function readSqliteTable(path: string, table: string): Promise<TableData> {
  return invoke<TableData>("read_sqlite_table", { path, table });
}
