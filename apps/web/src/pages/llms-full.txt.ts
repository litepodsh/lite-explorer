import type { APIRoute } from "astro";
import { faq, featureList, highlights, keys, legal, preview, site } from "../lib/content";

export const prerender = true;

export const GET: APIRoute = () => {
  const body = `# ${site.name} — full reference

${site.description}

Tagline: ${site.tagline}
Version: ${site.version} · License: ${site.license} · Publisher: ${legal.publisher} (${legal.email})
Platforms: ${site.platforms.join(", ")} · Stack: ${site.stack}
Website: ${site.url}/ · Source: ${site.repo} · Download: ${site.download}

## Features

${featureList.map((f) => `- ${f}`).join("\n")}

## Highlights

${highlights.map((h) => `### ${h.title} (${h.tag})\n${h.body}`).join("\n\n")}

## Built-in previews

${preview.map((p) => `- ${p}`).join("\n")}

## Keyboard shortcuts (macOS notation)

${keys.map((k) => `- ${k.combo.join(" ")}: ${k.label}`).join("\n")}

## Install

Download the build for your platform from ${site.download}. On macOS, open the universal DMG and drag Lite Explorer to Applications. Builds use a self-signed certificate and are not notarized by Apple: if macOS blocks the DMG, Control-click it, choose Open, then Open again.

## FAQ

${faq.map((item) => `### ${item.q}\n${item.a}`).join("\n\n")}
`;
  return new Response(body, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
