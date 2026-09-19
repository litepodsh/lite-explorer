import { describe, expect, test } from "bun:test";
import {
  availableCompressFormats,
  compressExtension,
  defaultArchiveName,
  isArchive,
} from "./archive.js";

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

describe("compress formats", () => {
  test("hides 7z when no binary is detected", () => {
    const values = availableCompressFormats(null).map((format) => format.value);
    expect(values).toEqual(["zip", "tar.gz", "tar"]);
  });

  test("offers 7z when a binary is detected", () => {
    const values = availableCompressFormats("/usr/bin/7z").map((format) => format.value);
    expect(values).toEqual(["zip", "7z", "tar.gz", "tar"]);
  });

  test("maps formats to extensions", () => {
    expect(compressExtension("zip")).toBe(".zip");
    expect(compressExtension("7z")).toBe(".7z");
    expect(compressExtension("tar.gz")).toBe(".tar.gz");
    expect(compressExtension("tar")).toBe(".tar");
  });
});

describe("defaultArchiveName", () => {
  test("uses the item name for a single target", () => {
    expect(defaultArchiveName(["report.pdf"])).toBe("report.pdf");
  });

  test("uses Archive for a multi-selection", () => {
    expect(defaultArchiveName(["a.txt", "b.txt"])).toBe("Archive");
  });
});
