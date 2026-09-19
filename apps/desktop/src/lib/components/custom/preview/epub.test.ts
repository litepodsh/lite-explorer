import { describe, expect, test } from "bun:test";
import { epubAssetUrl, resolveInner } from "./epub.js";

describe("epubAssetUrl", () => {
  test("encodes each inner segment but keeps separators", () => {
    expect(epubAssetUrl("media://localhost/abc", "OEBPS/img/a b.png")).toBe(
      "media://localhost/abc/OEBPS/img/a%20b.png",
    );
  });
});

describe("resolveInner", () => {
  test("resolves relative paths against the chapter directory", () => {
    expect(resolveInner("OEBPS/text/cap1.xhtml", "../images/a.png")).toBe("OEBPS/images/a.png");
    expect(resolveInner("OEBPS/cap0.xhtml", "img/a.png")).toBe("OEBPS/img/a.png");
  });

  test("drops fragments and ignores absolute or external references", () => {
    expect(resolveInner("OEBPS/cap0.xhtml", "cap1.xhtml#start")).toBe("OEBPS/cap1.xhtml");
    expect(resolveInner("OEBPS/cap0.xhtml", "#note")).toBe(null);
    expect(resolveInner("OEBPS/cap0.xhtml", "https://example.com/x")).toBe(null);
    expect(resolveInner("OEBPS/cap0.xhtml", "data:image/png;base64,AAAA")).toBe(null);
  });
});
