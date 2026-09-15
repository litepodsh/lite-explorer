import { detectDelimiter } from "./rainbow-csv.js";

// Row/column ceilings bound memory for multi-megabyte files; the table view only
// renders a window of rows anyway, but parsing everything would still cost.
export const MAX_ROWS = 50000;
export const MAX_COLUMNS = 512;
const DETECT_LINES = 50;

export type ParsedCsv = {
  delimiter: string;
  rows: string[][];
  truncated: boolean;
};

export function detectDelimiterFromText(text: string): string {
  const lines: string[] = [];
  for (const line of text.split("\n")) {
    lines.push(line);
    if (lines.length >= DETECT_LINES) break;
  }
  return detectDelimiter(lines);
}

// RFC 4180-ish parser: honors quoted fields, escaped "" quotes and newlines that
// live inside quotes. Column counts may vary per row; callers pad as needed.
export function parseCsv(
  text: string,
  options: { delimiter?: string; maxRows?: number; maxColumns?: number } = {},
): ParsedCsv {
  const delimiter = options.delimiter ?? detectDelimiterFromText(text);
  const maxRows = options.maxRows ?? MAX_ROWS;
  const maxColumns = options.maxColumns ?? MAX_COLUMNS;
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let inQuotes = false;
  let truncated = false;

  const endCell = () => {
    if (row.length < maxColumns) row.push(cell);
    else truncated = true;
    cell = "";
  };
  const endRow = () => {
    endCell();
    rows.push(row);
    row = [];
  };

  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (inQuotes) {
      if (char === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        cell += char;
      }
      continue;
    }
    if (char === '"') {
      inQuotes = true;
      continue;
    }
    if (char === delimiter) {
      endCell();
      continue;
    }
    if (char === "\n" || char === "\r") {
      if (char === "\r" && text[i + 1] === "\n") continue;
      if (rows.length >= maxRows) {
        truncated = true;
        break;
      }
      endRow();
      continue;
    }
    cell += char;
  }

  if (!truncated && (cell.length > 0 || row.length > 0)) {
    if (rows.length >= maxRows) truncated = true;
    else endRow();
  }
  return { delimiter, rows, truncated };
}
