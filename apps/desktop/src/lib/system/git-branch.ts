import { invoke } from "@tauri-apps/api/core";

/** Short-lived cache, so moving between folders of one repository reads `HEAD` once. */
const MAX_AGE_MS = 5_000;
const cache = new Map<string, { at: number; branch: Promise<string | null> }>();

/** Only plain local paths are looked up; remote and network listings have no `.git` to read. */
function isLocalPath(path: string): boolean {
  return path.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(path);
}

/** The Git branch of the repository that contains `path`, or null outside one. */
export function gitBranch(path: string): Promise<string | null> {
  if (!path || !isLocalPath(path)) return Promise.resolve(null);
  const cached = cache.get(path);
  if (cached && Date.now() - cached.at < MAX_AGE_MS) return cached.branch;
  const branch = invoke<string | null>("git_branch", { path }).catch(() => null);
  cache.set(path, { at: Date.now(), branch });
  if (cache.size > 64) cache.delete(cache.keys().next().value!);
  return branch;
}
