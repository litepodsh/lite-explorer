import { activity } from "$lib/transfers/jobs.js";
import { invoke } from "@tauri-apps/api/core";

const ARCHIVE_EXTENSIONS = [".zip", ".tar.gz", ".tgz", ".tar"];

export function isArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

export function createArchive(paths: string[], destination: string): Promise<void> {
  return activity.track("compress", `Compress ${paths.length} items`, destination, () => invoke<void>("create_archive", { paths, destination }));
}

export type ArchiveEntry = { path: string; isDirectory: boolean; size: number; skipped: boolean };
export type ArchiveListing = { entries: ArchiveEntry[]; uncompressedSize: number; truncated: boolean };
export type ExtractionTarget = {
  name: string;
  isDirectory: boolean;
  exists: boolean;
  existingModified: number | null;
};
export type Resolution = "replace" | "keepBoth" | "skip";
export type ExtractionOutcome = {
  extracted: string[];
  failed: { name: string; error: string }[];
  cancelled: boolean;
};
export type ExtractionJobRequest = {
  jobId: string;
  archive: string;
  destination: string;
  entries?: string[];
  resolutions: Record<string, Resolution>;
};

export function listArchive(path: string): Promise<ArchiveListing> {
  return invoke("list_archive", { path });
}

/** Targets the extraction would write, and which of them already exist. */
export function planExtraction(archive: string, destination: string, entries?: string[]): Promise<ExtractionTarget[]> {
  return invoke("plan_extraction", { archive, destination, entries: entries ?? null });
}

/** Resolves when the job ends. Progress arrives as `transfer-progress` events with the same job id. */
export function extractArchiveJob(request: ExtractionJobRequest): Promise<ExtractionOutcome> {
  return invoke("extract_archive", { ...request, entries: request.entries ?? null });
}
