import { describe, expect, test } from "bun:test";
import { baseName, parentPath } from "./files.js";

describe("parentPath", () => {
  test("local paths", () => {
    expect(parentPath("/Users/me/notes.txt")).toBe("/Users/me/");
    expect(parentPath("/Users/me/Documents")).toBe("/Users/me/");
  });

  test("remote folders end in a slash", () => {
    expect(parentPath("s3://id/media/photos/2026/")).toBe("s3://id/media/photos/");
    expect(parentPath("s3://id/media/photos/cat.png")).toBe("s3://id/media/photos/");
  });

  test("Windows paths use backslashes", () => {
    expect(parentPath("C:\\Users\\me\\notes.txt")).toBe("C:\\Users\\me\\");
    expect(parentPath("C:\\Users\\me\\Documents\\")).toBe("C:\\Users\\me\\");
    expect(parentPath("C:\\Users")).toBe("C:\\");
    expect(parentPath("C:\\")).toBe("");
  });
});

describe("baseName", () => {
  test("last segment of local, remote and Windows paths", () => {
    expect(baseName("/Users/me/Documents")).toBe("Documents");
    expect(baseName("/Users/me/Documents/")).toBe("Documents");
    expect(baseName("s3://id/media/photos/")).toBe("photos");
    expect(baseName("C:\\Users\\me")).toBe("me");
    expect(baseName("C:\\Users\\me\\")).toBe("me");
  });

  test("roots and empty paths", () => {
    expect(baseName("/")).toBe("");
    expect(baseName("C:\\")).toBe("C:");
    expect(baseName("")).toBe("");
  });
});
