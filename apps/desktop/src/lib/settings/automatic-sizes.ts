import type { Settings } from "./settings.js";

/** Compare whole path components; a sibling with the same prefix is not a descendant. */
export function isWithinFolder(path: string, folder: string): boolean {
  if (!path || !folder) return false;
  const normalize = (value: string) => {
    const windows = /^[a-z]:[\\/]/i.test(value) || value.startsWith("\\\\");
    const separated = windows ? value.replaceAll("\\", "/").toLowerCase() : value;
    return separated.replace(/\/+$/, "") || "/";
  };
  const root = normalize(folder);
  const target = normalize(path);
  return target === root || target.startsWith(root === "/" ? root : `${root}/`);
}

export function shouldCalculateSizes(
  path: string,
  home: string,
  settings: Pick<Settings, "automaticSizesInHome" | "automaticSizePaths">,
): boolean {
  return (
    (settings.automaticSizesInHome && isWithinFolder(path, home)) ||
    settings.automaticSizePaths.some((folder) => isWithinFolder(path, folder))
  );
}
