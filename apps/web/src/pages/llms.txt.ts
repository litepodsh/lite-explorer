import type { APIRoute } from "astro";
import { faq, featureList, legal, site } from "../lib/content";

export const prerender = true;

export const GET: APIRoute = () => {
  const body = `# ${site.name}

> ${site.description}

${site.name} is ${site.version}, released under the ${site.license} license by ${legal.publisher}. Platforms: ${site.platforms.join(", ")}. Built with ${site.stack}.

## Key facts

${featureList.map((f) => `- ${f}`).join("\n")}

## Links

- [Website](${site.url}/): overview, features, install and FAQ
- [Download](${site.download}): latest release builds
- [Source code](${site.repo}): GitHub repository
- [Changelog](${site.changelog}): release notes
- [Roadmap](${site.roadmap}): what is planned
- [Privacy policy](${site.url}${legal.privacyPath})
- [Terms of use](${site.url}${legal.termsPath})
- [Full text for LLMs](${site.url}/llms-full.txt)

## FAQ

${faq.map((item) => `### ${item.q}\n${item.a}`).join("\n\n")}
`;
  return new Response(body, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
};
