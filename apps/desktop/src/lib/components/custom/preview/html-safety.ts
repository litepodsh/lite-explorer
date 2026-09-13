export type HtmlSafety = {
  level: "safe" | "caution";
  reasons: string[];
};

const EMBEDDED_TAGS = new Set(["iframe", "object", "embed", "frame", "frameset"]);

const EXTERNAL_ATTRIBUTES = [
  "src",
  "href",
  "action",
  "poster",
  "data",
  "background",
  "cite",
  "codebase",
];

const TAG_PATTERN = /<([a-zA-Z][\w-]*)\b([^>]*)>/g;
const ATTRIBUTE_PATTERN = /(?:^|\s)([a-zA-Z_:][\w:.-]*)(\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]*)))?/gi;
const EXTERNAL_VALUE_PATTERN = /^(https?:|\/\/)/i;

function attributeValue(match: RegExpExecArray): string {
  return match[4] ?? match[5] ?? match[6] ?? "";
}

export function analyzeHtmlSafety(html: string): HtmlSafety {
  const reasons: string[] = [];

  if (/<script\b/i.test(html)) reasons.push("Runs JavaScript");

  TAG_PATTERN.lastIndex = 0;
  let tag: RegExpExecArray | null;
  while ((tag = TAG_PATTERN.exec(html)) !== null) {
    const tagName = tag[1].toLowerCase();
    const attrs = tag[2];

    if (EMBEDDED_TAGS.has(tagName)) {
      if (!reasons.includes("Embeds another page")) reasons.push("Embeds another page");
    }
    if (tagName === "form" && /\baction\s*=/i.test(attrs)) {
      if (!reasons.includes("Submits data")) reasons.push("Submits data");
    }

    ATTRIBUTE_PATTERN.lastIndex = 0;
    let attr: RegExpExecArray | null;
    while ((attr = ATTRIBUTE_PATTERN.exec(attrs)) !== null) {
      const name = attr[1].toLowerCase();
      const value = attributeValue(attr);
      if (name.startsWith("on")) {
        if (!reasons.includes("Inline event handlers")) reasons.push("Inline event handlers");
      }
      if (/^javascript:/i.test(value)) {
        if (!reasons.includes("javascript: URLs")) reasons.push("javascript: URLs");
      }
      if (EXTERNAL_ATTRIBUTES.includes(name) && EXTERNAL_VALUE_PATTERN.test(value.trim())) {
        if (!reasons.includes("Loads external resources")) reasons.push("Loads external resources");
      }
    }
  }

  return {
    level: reasons.length === 0 ? "safe" : "caution",
    reasons,
  };
}
