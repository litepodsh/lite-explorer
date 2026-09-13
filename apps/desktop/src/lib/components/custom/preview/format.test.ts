import { describe, expect, test } from "bun:test";
import { formatSize } from "./format.js";

describe("formatSize", () => {
  test("uses bytes below 1000", () => {
    expect(formatSize(0)).toBe("0 bytes");
    expect(formatSize(1)).toBe("1 byte");
    expect(formatSize(743)).toBe("743 bytes");
  });

  test("uses decimal units with one decimal", () => {
    expect(formatSize(1000)).toBe("1 KB");
    expect(formatSize(1234)).toBe("1.2 KB");
    expect(formatSize(3_400_000)).toBe("3.4 MB");
    expect(formatSize(5_000_000_000)).toBe("5 GB");
  });
});
