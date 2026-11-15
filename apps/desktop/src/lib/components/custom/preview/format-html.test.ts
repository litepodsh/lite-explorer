import { describe, expect, test } from "bun:test";
import { formatHtml } from "./format-html.js";

describe("formatHtml", () => {
  test("puts nested tags, inline styles and scripts on their own lines", async () => {
    const pretty = await formatHtml(
      '<html><head><style>body{margin:0}</style></head><body><ul><li>One</li></ul><script>let a=1</script></body></html>',
    );
    expect(pretty).toContain("\n    <ul>\n      <li>One</li>\n    </ul>");
    expect(pretty).toContain("body {\n        margin: 0;\n      }");
    expect(pretty).toContain("let a = 1;");
  });

  test("returns null for empty or oversized sources", async () => {
    expect(await formatHtml("   ")).toBeNull();
    expect(await formatHtml("<p>x</p>".repeat(70_000))).toBeNull();
  });
});
