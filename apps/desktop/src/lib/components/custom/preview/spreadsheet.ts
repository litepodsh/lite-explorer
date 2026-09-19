import { invoke } from "@tauri-apps/api/core";

export type SheetTable = {
  name: string;
  rows: string[][];
  truncated: boolean;
};

export type SpreadsheetData = {
  sheets: SheetTable[];
  truncated: boolean;
};

/** Parses a local, remote or server workbook into a bounded grid per sheet. */
export function readSpreadsheet(path: string): Promise<SpreadsheetData> {
  return invoke<SpreadsheetData>("read_spreadsheet", { path });
}
