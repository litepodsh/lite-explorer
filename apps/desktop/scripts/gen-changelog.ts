import { parseChangelog } from "../src/lib/updates/parse-changelog.js";

const root = new URL("../../../changelog/CHANGELOG.md", import.meta.url);
const out = new URL("../src/lib/updates/changelog.generated.json", import.meta.url);

const source = await Bun.file(root).text();
const releases = parseChangelog(source).filter((release) => release.version !== "Unreleased");

if (!releases.length) throw new Error("CHANGELOG.md has no released version");

const pkg = await Bun.file(new URL("../package.json", import.meta.url)).json();
const latest = releases[0].version;
if (latest !== pkg.version) {
  console.warn(
    `CHANGELOG.md top version is ${latest} but package.json is ${pkg.version}. Add a section for ${pkg.version}.`,
  );
}

await Bun.write(out, `${JSON.stringify({ releases }, null, 2)}\n`);
console.log(`Wrote ${releases.length} releases, latest ${latest}`);
