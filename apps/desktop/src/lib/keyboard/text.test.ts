import { describe, expect, test } from "bun:test";
import { fileStem, withTrailingSlash } from "./text.js";

describe("fileStem", () => {
  test("removes the last extension only", () => {
    expect(fileStem("report.final.pdf")).toBe("report.final");
    expect(fileStem("README")).toBe("README");
    expect(fileStem(".bashrc")).toBe(".bashrc");
  });
});

describe("withTrailingSlash", () => {
  test("adds one separator", () => {
    expect(withTrailingSlash("/Users/me")).toBe("/Users/me/");
    expect(withTrailingSlash("/Users/me/")).toBe("/Users/me/");
    expect(withTrailingSlash("C:\\Users\\")).toBe("C:\\Users\\");
    expect(withTrailingSlash("")).toBe("/");
  });
});
