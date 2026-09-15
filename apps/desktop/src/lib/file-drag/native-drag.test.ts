import { describe, expect, test } from "bun:test";
import { atWindowEdge, canDragOut, crc32, pngWithDensity } from "./native-drag";

const bytes = (text: string) => Uint8Array.from(text, (character) => character.charCodeAt(0));

describe("crc32", () => {
  test("matches the CRC of a PNG IEND chunk", () => {
    expect(crc32(bytes("IEND"))).toBe(0xae426082);
  });
});

describe("pngWithDensity", () => {
  test("inserts a pHYs chunk right after IHDR", () => {
    const png = new Uint8Array(33 + 12).map((_, index) => index);
    const result = pngWithDensity(png, 2);
    const view = new DataView(result.buffer);
    expect(result.length).toBe(png.length + 21);
    expect(Array.from(result.subarray(0, 33))).toEqual(Array.from(png.subarray(0, 33)));
    expect(view.getUint32(33)).toBe(9);
    expect(String.fromCharCode(...result.subarray(37, 41))).toBe("pHYs");
    expect(view.getUint32(41)).toBe(5669);
    expect(view.getUint32(45)).toBe(5669);
    expect(result[49]).toBe(1);
    expect(view.getUint32(50)).toBe(crc32(result.subarray(37, 50)));
    expect(Array.from(result.subarray(54))).toEqual(Array.from(png.subarray(33)));
  });
});

describe("atWindowEdge", () => {
  test("inside the window", () => {
    expect(atWindowEdge(200, 150, 800, 600)).toBe(false);
  });

  test("at or past any edge", () => {
    expect(atWindowEdge(0, 150, 800, 600)).toBe(true);
    expect(atWindowEdge(200, -3, 800, 600)).toBe(true);
    expect(atWindowEdge(799, 150, 800, 600)).toBe(true);
    expect(atWindowEdge(200, 612, 800, 600)).toBe(true);
  });
});

describe("canDragOut", () => {
  test("local files and folders", () => {
    expect(canDragOut({ path: "/Users/me/photo.png" })).toBe(true);
  });

  test("remote paths, buckets and shares stay in the app", () => {
    expect(canDragOut({ path: "s3://id/bucket/photo.png" })).toBe(false);
    expect(canDragOut({ path: "sftp://id/home/notes.txt" })).toBe(false);
    expect(canDragOut({ path: "smb://id/share", kind: "share" })).toBe(false);
  });
});
