import { expect, test } from "bun:test";
import { externalUrl } from "./external-link.js";

test("only accepts http and https URLs", () => {
  expect(externalUrl("https://example.com")).toBe("https://example.com/");
  expect(externalUrl("javascript:alert(1)")).toBeNull();
  expect(externalUrl("file:///secret")).toBeNull();
});
