// Sets the app version everywhere it's declared: `bun scripts/bump-version.ts 0.2.0`.
// The release workflow builds from a `v<version>` tag, so tag the commit after bumping.

const version = process.argv[2]?.replace(/^v/, "");
if (!version || !/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) {
  console.error("Usage: just bump 1.2.3");
  process.exit(1);
}

async function updateJson(path: string) {
  const data = JSON.parse(await Bun.file(path).text());
  const previous = data.version;
  data.version = version;
  await Bun.write(path, JSON.stringify(data, null, 2) + "\n");
  return previous;
}

async function updateCargo(path: string) {
  const text = await Bun.file(path).text();
  // Only the first `version` line, which belongs to [package].
  const next = text.replace(/^version = ".*"$/m, `version = "${version}"`);
  if (next === text && !text.includes(`version = "${version}"`)) {
    throw new Error(`No package version found in ${path}`);
  }
  await Bun.write(path, next);
}

const previous = await updateJson("src-tauri/tauri.conf.json");
await updateJson("package.json");
await updateCargo("src-tauri/Cargo.toml");

console.log(`Version ${previous} → ${version}`);
console.log("Next: commit, then `git tag v" + version + " && git push origin v" + version + "`.");
