import { describe, expect, test } from "bun:test";
import { parentPath } from "./files.js";

describe("parentPath", () => {
  test("local paths", () => {
    expect(parentPath("/Users/me/notes.txt")).toBe("/Users/me/");
    expect(parentPath("/Users/me/Documents")).toBe("/Users/me/");
  });

  test("remote folders end in a slash", () => {
    expect(parentPath("s3://id/media/photos/2026/")).toBe("s3://id/media/photos/");
    expect(parentPath("s3://id/media/photos/cat.png")).toBe("s3://id/media/photos/");
  });
});
