import { describe, expect, test } from "bun:test";
import { sizeSegments } from "./segments.js";

describe("sizeSegments", () => {
  test("sizes each segment as a share of the total", () => {
    const sized = sizeSegments(
      [
        { id: "home", value: 640 },
        { id: "system", value: 150 },
      ],
      1000,
    );
    expect(sized.map((segment) => segment.percent)).toEqual([64, 15]);
  });

  test("caps the sum at 100 when values overshoot the total", () => {
    const sized = sizeSegments(
      [
        { id: "a", value: 80 },
        { id: "b", value: 50 },
        { id: "c", value: 10 },
      ],
      100,
    );
    expect(sized.map((segment) => segment.percent)).toEqual([80, 20, 0]);
  });

  test("returns zero widths for an empty total or negative values", () => {
    expect(sizeSegments([{ id: "a", value: 5 }], 0)[0].percent).toBe(0);
    expect(sizeSegments([{ id: "a", value: -5 }], 10)[0].percent).toBe(0);
  });
});
