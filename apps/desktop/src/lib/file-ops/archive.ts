import { activity } from "$lib/transfers/jobs.js";
import { invoke } from "@tauri-apps/api/core";

const ARCHIVE_EXTENSIONS = [".zip", ".tar.gz", ".tgz", ".tar"];

export function isArchive(name: string): boolean {
  const lower = name.toLowerCase();
  return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

export function createArchive(paths: string[], destination: string): Promise<void> {
  return activity.track("compress", `Compress ${paths.length} items`, destination, () =>
    invoke<void>("create_archive", { paths, destination }),
  );
}

export type CompressFormat = "zip" | "7z" | "tar.gz" | "tar";

export type CompressFormatOption = {
  value: CompressFormat;
  label: string;
  extension: string;
  /** Only offered when the external 7-Zip binary is installed. */
  needsSevenZip?: boolean;
};

export const COMPRESS_FORMATS: readonly CompressFormatOption[] = [
  { value: "zip", label: "ZIP", extension: ".zip" },
  { value: "7z", label: "7-Zip", extension: ".7z", needsSevenZip: true },
  { value: "tar.gz", label: "Tarball (gzip)", extension: ".tar.gz" },
  { value: "tar", label: "Tarball (tar)", extension: ".tar" },
];

/** Formats the dialog can offer, given the detected 7-Zip path (or `null`). */
export function availableCompressFormats(sevenZip: string | null): CompressFormatOption[] {
  return COMPRESS_FORMATS.filter((format) => !format.needsSevenZip || sevenZip !== null);
}

export function compressExtension(format: CompressFormat): string {
  return COMPRESS_FORMATS.find((option) => option.value === format)?.extension ?? ".zip";
}

/** Suggested archive name: the item's own name, or `Archive` for a multi-selection. */
export function defaultArchiveName(names: readonly string[]): string {
  if (names.length === 1) return names[0];
  return "Archive";
}

/** Path of the external 7-Zip binary, or `null` when it is not installed. */
export function detectSevenZip(): Promise<string | null> {
  return invoke<string | null>("detect_7z");
}

export function pathExists(path: string): Promise<boolean> {
  return invoke<boolean>("path_exists", { path });
}

export type ArchiveEntry = { path: string; isDirectory: boolean; size: number; skipped: boolean };
export type ArchiveListing = {
  entries: ArchiveEntry[];
  uncompressedSize: number;
  truncated: boolean;
};
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
export function planExtraction(
  archive: string,
  destination: string,
  entries?: string[],
): Promise<ExtractionTarget[]> {
  return invoke("plan_extraction", { archive, destination, entries: entries ?? null });
}

/** Resolves when the job ends. Progress arrives as `transfer-progress` events with the same job id. */
export function extractArchiveJob(request: ExtractionJobRequest): Promise<ExtractionOutcome> {
  return invoke("extract_archive", { ...request, entries: request.entries ?? null });
}
