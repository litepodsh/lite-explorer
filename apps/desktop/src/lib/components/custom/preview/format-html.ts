/** Larger documents stay as written: formatting them would stall the preview. */
const MAX_FORMAT_CHARS = 512 * 1024;
const cache = new Map<string, Promise<string | null>>();

/**
 * Pretty prints HTML, with its inline `<style>` and `<script>`, for the code view.
 * Prettier loads on first use. Null when the source cannot be parsed or is too large.
 */
export function formatHtml(source: string): Promise<string | null> {
  if (!source.trim() || source.length > MAX_FORMAT_CHARS) return Promise.resolve(null);
  const cached = cache.get(source);
  if (cached) return cached;
  const result = run(source);
  cache.set(source, result);
  if (cache.size > 16) cache.delete(cache.keys().next().value!);
  return result;
}

async function run(source: string): Promise<string | null> {
  try {
    const [prettier, html, postcss, babel, estree] = await Promise.all([
      import("prettier/standalone"),
      import("prettier/plugins/html"),
      import("prettier/plugins/postcss"),
      import("prettier/plugins/babel"),
      import("prettier/plugins/estree"),
    ]);
    return await prettier.format(source, {
      parser: "html",
      plugins: [html, postcss, babel, estree],
      printWidth: 100,
      // Whitespace in inline content is preserved as the browser treats it; "ignore" reflows freely.
      htmlWhitespaceSensitivity: "ignore",
    });
  } catch {
    return null;
  }
}
