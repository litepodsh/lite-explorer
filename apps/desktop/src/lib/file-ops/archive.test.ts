import { describe, expect, test } from "bun:test";
import { isArchive } from "./archive.js";

describe("isArchive", () => {
  test("recognizes supported archive extensions, case-insensitively", () => {
    expect(isArchive("Photos.zip")).toBe(true);
    expect(isArchive("backup.tar")).toBe(true);
    expect(isArchive("backup.tar.gz")).toBe(true);
    expect(isArchive("bundle.TGZ")).toBe(true);
  });

  test("rejects other files", () => {
    expect(isArchive("notes.md")).toBe(false);
    expect(isArchive("archive.rar")).toBe(false);
    expect(isArchive("zip")).toBe(false);
  });
});
