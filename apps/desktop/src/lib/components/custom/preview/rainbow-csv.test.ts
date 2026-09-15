import { describe, expect, test } from "bun:test";
import { columnClass, detectDelimiter, lineDecorations, scanLine } from "./rainbow-csv.js";

describe("detectDelimiter", () => {
  test("detects commas", () => {
    expect(detectDelimiter(["a,b,c", "1,2,3"])).toBe(",");
  });

  test("detects tabs", () => {
    expect(detectDelimiter(["a\tb\tc", "1\t2\t3"])).toBe("\t");
  });

  test("detects semicolons", () => {
    expect(detectDelimiter(["a;b;c", "1;2;3"])).toBe(";");
  });

  test("detects pipes", () => {
    expect(detectDelimiter(["a|b|c", "1|2|3"])).toBe("|");
  });

  test("prefers the delimiter with more consistent columns", () => {
    const lines = ["id,name,city", "1,Ada,Paris", "2,Alan,London", "3,Grace,New York"];
    expect(detectDelimiter(lines)).toBe(",");
  });

  test("ignores delimiters inside quoted fields", () => {
    const lines = ['"a,b",c,d', '"1,2",3,4', '"x,y",5,6'];
    expect(detectDelimiter(lines)).toBe(",");
  });

  test("falls back to comma when there is no delimiter", () => {
    expect(detectDelimiter(["single column", "another row"])).toBe(",");
    expect(detectDelimiter([])).toBe(",");
  });
});

describe("scanLine", () => {
  test("splits plain cells and records delimiter spans", () => {
    const { cells, delimiters } = scanLine("a,b,c", ",");
    expect(cells.map((cell) => cell.column)).toEqual([0, 1, 2]);
    expect(delimiters.map((span) => span.start)).toEqual([1, 3]);
  });

  test("does not split on a delimiter inside quotes", () => {
    const { cells, delimiters } = scanLine('"a,b",c', ",");
    expect(cells.map((cell) => [cell.start, cell.end])).toEqual([
      [0, 5],
      [6, 7],
    ]);
    expect(delimiters.map((span) => span.start)).toEqual([5]);
  });

  test("treats doubled quotes as an escaped quote", () => {
    const { delimiters } = scanLine('"he said ""hi"", ok",x', ",");
    expect(delimiters.length).toBe(1);
    expect(delimiters[0].start).toBe(20);
  });

  test("keeps an empty trailing cell", () => {
    const { cells } = scanLine("a,b,", ",");
    expect(cells.map((cell) => cell.column)).toEqual([0, 1, 2]);
    expect(cells[2].end).toBe(cells[2].start);
  });
});

describe("columnClass", () => {
  test("cycles the palette", () => {
    expect(columnClass(0)).toBe("rainbow-col-0");
    expect(columnClass(10)).toBe("rainbow-col-0");
    expect(columnClass(13)).toBe("rainbow-col-3");
  });
});

describe("lineDecorations", () => {
  test("colors each cell and dims delimiters", () => {
    const decorations = lineDecorations(1, "a,b", ",");
    expect(decorations.length).toBe(3);
    expect(decorations[0].options.inlineClassName).toBe("rainbow-delim");
    expect(decorations[1].options.inlineClassName).toBe("rainbow-col-0");
    expect(decorations[2].options.inlineClassName).toBe("rainbow-col-1");
  });

  test("uses Monaco's 1-based columns", () => {
    const decorations = lineDecorations(4, "a,b", ",");
    const firstCell = decorations.find(
      (decoration) => decoration.options.inlineClassName === "rainbow-col-0",
    );
    expect(firstCell?.range).toEqual({
      startLineNumber: 4,
      startColumn: 1,
      endLineNumber: 4,
      endColumn: 2,
    });
  });

  test("skips empty cells and empty lines", () => {
    expect(lineDecorations(1, "", ",").length).toBe(0);
    const decorations = lineDecorations(1, "a,", ",");
    expect(decorations.length).toBe(2);
  });
});
