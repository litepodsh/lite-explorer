import { invoke } from "@tauri-apps/api/core";

const ARCHIVE_EXTENSIONS = [".zip", ".tar.gz", ".tgz", ".tar"];

export function isArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

export function createArchive(paths: string[], destination: string): Promise<void> {
  return invoke("create_archive", { paths, destination });
}

export function extractArchive(path: string, destination: string): Promise<void> {
  return invoke("extract_archive", { path, destination });
}
