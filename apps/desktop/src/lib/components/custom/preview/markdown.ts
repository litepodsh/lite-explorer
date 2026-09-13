import DOMPurify from "dompurify";
import { marked } from "marked";

const LOCAL_IMAGE_ATTRIBUTE = "data-local-image";

// Relative images need the Tauri asset protocol, which is out of scope. The src
// is removed while the node is still in DOMPurify's inert document, so the
// webview never requests it.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.nodeName !== "IMG") return;
  const image = node as Element;
  const source = image.getAttribute("src") ?? "";
  if (/^(https?:|data:)/i.test(source)) return;
  image.removeAttribute("src");
  image.removeAttribute("srcset");
  image.setAttribute(LOCAL_IMAGE_ATTRIBUTE, "");
});

export function renderMarkdown(source: string): DocumentFragment {
  const html = marked.parse(source, { async: false, gfm: true });
  const fragment = DOMPurify.sanitize(html, { RETURN_DOM_FRAGMENT: true });
  for (const image of fragment.querySelectorAll(`img[${LOCAL_IMAGE_ATTRIBUTE}]`)) {
    const label = document.createElement("span");
    label.className = "markdown-missing-image";
    label.textContent = image.getAttribute("alt") ?? "";
    image.replaceWith(label);
  }
  return fragment;
}
