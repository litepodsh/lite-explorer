import { describe, expect, test } from "bun:test";
import { detectDelimiterFromText, parseCsv } from "./csv.js";

describe("detectDelimiterFromText", () => {
  test("detects tabs from a TSV sample", () => {
    expect(detectDelimiterFromText("a\tb\tc\n1\t2\t3")).toBe("\t");
  });

  test("detects pipes from a PSV sample", () => {
    expect(detectDelimiterFromText("a|b|c\n1|2|3")).toBe("|");
  });
});

describe("parseCsv", () => {
  test("parses rows and cells", () => {
    const { rows, delimiter } = parseCsv("a,b,c\n1,2,3");
    expect(delimiter).toBe(",");
    expect(rows).toEqual([
      ["a", "b", "c"],
      ["1", "2", "3"],
    ]);
  });

  test("strips surrounding quotes and keeps delimiters inside them", () => {
    const { rows } = parseCsv('"last, first",age\n"Doe, Jane",42');
    expect(rows).toEqual([
      ["last, first", "age"],
      ["Doe, Jane", "42"],
    ]);
  });

  test("unescapes doubled quotes", () => {
    const { rows } = parseCsv('"he said ""hi""",x');
    expect(rows).toEqual([['he said "hi"', "x"]]);
  });

  test("keeps newlines inside quoted fields", () => {
    const { rows } = parseCsv('a,"line1\nline2",b');
    expect(rows).toEqual([["a", "line1\nline2", "b"]]);
  });

  test("handles CRLF and ignores a trailing newline", () => {
    const { rows } = parseCsv("a,b\r\n1,2\r\n");
    expect(rows).toEqual([
      ["a", "b"],
      ["1", "2"],
    ]);
  });

  test("keeps empty trailing cells", () => {
    const { rows } = parseCsv("a,b,\n");
    expect(rows).toEqual([["a", "b", ""]]);
  });

  test("marks the result truncated when it hits the row cap", () => {
    const { rows, truncated } = parseCsv("a\nb\nc", { maxRows: 2 });
    expect(truncated).toBe(true);
    expect(rows.length).toBe(2);
  });

  test("accepts an explicit delimiter", () => {
    const { rows } = parseCsv("a;b\n1;2", { delimiter: ";" });
    expect(rows).toEqual([
      ["a", "b"],
      ["1", "2"],
    ]);
  });
});
