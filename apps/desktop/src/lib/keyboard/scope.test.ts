import { describe, expect, test } from "bun:test";
import { scopeFromFacts, type ScopeFacts } from "./scope.js";

const facts = (overrides: Partial<ScopeFacts> = {}): ScopeFacts => ({
  dialog: false,
  monaco: false,
  input: false,
  control: false,
  region: null,
  ...overrides,
});

describe("scopeFromFacts", () => {
  test("dialog wins over everything", () => {
    expect(scopeFromFacts(facts({ dialog: true, input: true, monaco: true, region: "list" }))).toBe("dialog");
  });

  test("monaco wins over its own text area", () => {
    expect(scopeFromFacts(facts({ monaco: true, input: true, region: "preview" }))).toBe("monaco");
  });

  test("text fields are input", () => {
    expect(scopeFromFacts(facts({ input: true, region: "list" }))).toBe("input");
  });

  test("a marked region wins over controls", () => {
    expect(scopeFromFacts(facts({ control: true, region: "sidebar" }))).toBe("sidebar");
  });

  test("unmarked controls are global", () => {
    expect(scopeFromFacts(facts({ control: true }))).toBe("global");
  });

  test("anything else is the file list", () => {
    expect(scopeFromFacts(facts())).toBe("list");
  });
});
