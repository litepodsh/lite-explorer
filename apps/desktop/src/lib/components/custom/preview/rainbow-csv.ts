import type * as Monaco from "monaco-editor/editor/editor.api";

// Rainbow CSV column highlighting, like the VS Code "Rainbow CSV" extension.
// Monaco has no per-column tokenizer, so colors are applied through inline
// decorations. Only the visible lines are scanned, which keeps huge files cheap.
//
// The CSS classes below live in `app.css`; keep the palette in sync.
export const RAINBOW_COLUMN_COUNT = 10;
export const DELIMITER_CLASS = "rainbow-delim";

export const DELIMITER_CANDIDATES = [",", "\t", ";", "|"] as const;

// Hard ceilings; also guard pathological one-line files.
const MAX_DECORATIONS = 6000;
const MAX_CELLS_PER_LINE = 2000;
const DETECT_SAMPLE_LINES = 50;

export type CellSpan = { start: number; end: number; column: number };
export type DelimiterSpan = { start: number; end: number };
export type ScannedLine = { cells: CellSpan[]; delimiters: DelimiterSpan[] };

export function columnClass(column: number): string {
  return `rainbow-col-${column % RAINBOW_COLUMN_COUNT}`;
}

// Splits a line on `delimiter`, ignoring delimiters inside quoted fields.
// `""` inside a quoted field is an escaped quote and does not toggle state.
export function scanLine(line: string, delimiter: string): ScannedLine {
  const cells: CellSpan[] = [];
  const delimiters: DelimiterSpan[] = [];
  let column = 0;
  let cellStart = 0;
  let inQuotes = false;

  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    if (char === '"') {
      if (inQuotes && line[i + 1] === '"') {
        i += 1;
        continue;
      }
      inQuotes = !inQuotes;
      continue;
    }
    if (!inQuotes && char === delimiter) {
      cells.push({ start: cellStart, end: i, column });
      delimiters.push({ start: i, end: i + 1 });
      column += 1;
      cellStart = i + 1;
    }
  }
  cells.push({ start: cellStart, end: line.length, column });
  return { cells, delimiters };
}

// Picks the delimiter with the highest consistent number of columns across the
// sample. Ties keep the earliest candidate, so comma wins on ambiguous files.
export function detectDelimiter(lines: string[]): string {
  let best: string = DELIMITER_CANDIDATES[0];
  let bestScore = -1;

  for (const delimiter of DELIMITER_CANDIDATES) {
    const counts: number[] = [];
    for (const line of lines) {
      if (!line) continue;
      const { delimiters } = scanLine(line, delimiter);
      if (delimiters.length > 0) counts.push(delimiters.length);
    }
    if (counts.length === 0) continue;

    const frequency = new Map<number, number>();
    for (const count of counts) frequency.set(count, (frequency.get(count) ?? 0) + 1);
    let mode = 0;
    let modeCount = 0;
    for (const [count, seen] of frequency) {
      if (seen > modeCount || (seen === modeCount && count > mode)) {
        mode = count;
        modeCount = seen;
      }
    }
    const score = mode * (modeCount / counts.length);
    if (score > bestScore) {
      bestScore = score;
      best = delimiter;
    }
  }
  return best;
}

export function sampleLines(model: Monaco.editor.ITextModel): string[] {
  const lines: string[] = [];
  const last = Math.min(model.getLineCount(), DETECT_SAMPLE_LINES);
  for (let line = 1; line <= last; line++) lines.push(model.getLineContent(line));
  return lines;
}

// Builds inline decorations for one line. Empty cells (trailing delimiters) are
// skipped so Monaco never receives zero-width ranges.
export function lineDecorations(
  lineNumber: number,
  content: string,
  delimiter: string,
): Monaco.editor.IModelDeltaDecoration[] {
  if (!content) return [];
  const { cells, delimiters } = scanLine(content, delimiter);
  const decorations: Monaco.editor.IModelDeltaDecoration[] = [];

  for (const span of delimiters) {
    decorations.push({
      range: {
        startLineNumber: lineNumber,
        startColumn: span.start + 1,
        endLineNumber: lineNumber,
        endColumn: span.end + 1,
      },
      options: { inlineClassName: DELIMITER_CLASS },
    });
  }
  for (const cell of cells) {
    if (cell.end <= cell.start || cell.column >= MAX_CELLS_PER_LINE) continue;
    decorations.push({
      range: {
        startLineNumber: lineNumber,
        startColumn: cell.start + 1,
        endLineNumber: lineNumber,
        endColumn: cell.end + 1,
      },
      options: { inlineClassName: columnClass(cell.column) },
    });
  }
  return decorations;
}

export function visibleDecorations(
  model: Monaco.editor.ITextModel,
  delimiter: string,
  visibleLines: Iterable<number>,
): Monaco.editor.IModelDeltaDecoration[] {
  const decorations: Monaco.editor.IModelDeltaDecoration[] = [];
  for (const lineNumber of visibleLines) {
    if (lineNumber < 1 || lineNumber > model.getLineCount()) continue;
    decorations.push(...lineDecorations(lineNumber, model.getLineContent(lineNumber), delimiter));
    if (decorations.length >= MAX_DECORATIONS) break;
  }
  return decorations.slice(0, MAX_DECORATIONS);
}
