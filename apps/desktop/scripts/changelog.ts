// Writes the next release section into changelog/CHANGELOG.md with git-cliff, from the
// conventional commits since the last tag. The version comes from package.json, so bump it
// first. The section goes under "## [Unreleased]"; edit it by hand before tagging if needed.
const changelog = new URL("../../../changelog/CHANGELOG.md", import.meta.url);
const pkg = await Bun.file(new URL("../package.json", import.meta.url)).json();
const version: string = pkg.version;

const source = await Bun.file(changelog).text();
if (source.includes(`## [${version}]`)) {
  throw new Error(`CHANGELOG.md already has a section for ${version}`);
}

const cliff = Bun.spawnSync(
  ["git-cliff", "--config", "cliff.toml", "--unreleased", "--tag", `v${version}`, "--strip", "all"],
  { cwd: `${import.meta.dir}/..`, stderr: "inherit" },
);
if (!cliff.success) throw new Error("git-cliff failed. Install it with `brew install git-cliff`.");
const section = cliff.stdout.toString().trim();

const marker = "## [Unreleased]";
if (!source.includes(marker)) throw new Error(`CHANGELOG.md has no "${marker}" heading`);
await Bun.write(changelog, source.replace(marker, `${marker}\n\n${section}`));
console.log(section);
