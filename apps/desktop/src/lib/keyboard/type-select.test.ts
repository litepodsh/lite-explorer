import { describe, expect, test } from "bun:test";
import { TypeSelectBuffer, typeSelectTarget } from "./type-select.js";

const names = ["Applications", "readme.md", "Reports", "Résumé.pdf", "src", "Zeta"];

describe("typeSelectTarget", () => {
  test("first name with the prefix, ignoring case", () => {
    expect(typeSelectTarget(names, "r", 0)).toBe(1);
    expect(typeSelectTarget(names, "REP", 0)).toBe(2);
    expect(typeSelectTarget(names, "z", 0)).toBe(5);
  });

  test("ignores accents", () => {
    expect(typeSelectTarget(names, "resu", 0)).toBe(3);
  });

  test("no match leaves focus alone", () => {
    expect(typeSelectTarget(names, "x", 0)).toBeNull();
    expect(typeSelectTarget(names, "", 0)).toBeNull();
    expect(typeSelectTarget([], "r", -1)).toBeNull();
  });

  test("repeating a letter cycles through its names", () => {
    expect(typeSelectTarget(names, "rr", 1)).toBe(2);
    expect(typeSelectTarget(names, "rrr", 2)).toBe(3);
    expect(typeSelectTarget(names, "rrrr", 3)).toBe(1);
  });
});

describe("TypeSelectBuffer", () => {
  test("joins quick keys and restarts after a pause", () => {
    const buffer = new TypeSelectBuffer(1000);
    expect(buffer.push("r", 5000)).toBe("r");
    expect(buffer.push("e", 5500)).toBe("re");
    expect(buffer.push("s", 7000)).toBe("s");
  });
});
