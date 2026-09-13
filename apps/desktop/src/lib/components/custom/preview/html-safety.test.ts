import { describe, expect, test } from "bun:test";
import { analyzeHtmlSafety } from "./html-safety.js";

describe("analyzeHtmlSafety", () => {
  test("marks static markup with inline CSS as safe", () => {
    const html = `<style>body{color:red}</style><h1>Hi</h1><p class="x">text</p>`;
    const result = analyzeHtmlSafety(html);
    expect(result.level).toBe("safe");
    expect(result.reasons).toEqual([]);
  });

  test("flags inline scripts", () => {
    const result = analyzeHtmlSafety(`<script>alert(1)</script>`);
    expect(result.level).toBe("caution");
    expect(result.reasons).toEqual(["Runs JavaScript"]);
  });

  test("flags inline event handlers", () => {
    const result = analyzeHtmlSafety(`<button onclick="x()">go</button>`);
    expect(result.reasons).toEqual(["Inline event handlers"]);
  });

  test("flags embedded frames", () => {
    const result = analyzeHtmlSafety(`<iframe src="x"></iframe>`);
    expect(result.reasons).toEqual(["Embeds another page"]);
  });

  test("flags form submissions", () => {
    const result = analyzeHtmlSafety(`<form action="/submit"><button>send</button></form>`);
    expect(result.reasons).toEqual(["Submits data"]);
  });

  test("flags javascript: URIs", () => {
    const result = analyzeHtmlSafety(`<a href="javascript:alert(1)">x</a>`);
    expect(result.reasons).toEqual(["javascript: URLs"]);
  });

  test("flags external network resources", () => {
    const result = analyzeHtmlSafety(`<img src="https://example.com/a.png">`);
    expect(result.reasons).toEqual(["Loads external resources"]);
  });

  test("collects multiple reasons", () => {
    const result = analyzeHtmlSafety(`<script>1</script><a href="https://x.com">y</a>`);
    expect(result.reasons).toEqual(["Runs JavaScript", "Loads external resources"]);
  });
});
