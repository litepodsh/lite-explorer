import { invoke } from "@tauri-apps/api/core";

export type ImageFormat = "png" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "ico" | "avif";

/** Mirror of `Resize` in `src-tauri/src/explorer/convert/transform.rs`. */
export type Resize =
  | { kind: "none" }
  | { kind: "percent"; value: number }
  | { kind: "fit"; width: number; height: number };

/** Mirror of `ConvertOptions` in `src-tauri/src/explorer/convert/job.rs`. */
export type ConvertOptions = {
  /** `null` keeps each file's own format. */
  format: ImageFormat | null;
  quality: number;
  resize: Resize;
  rotate: 0 | 90 | 180 | 270;
  flipH: boolean;
  flipV: boolean;
  stripMetadata: boolean;
  suffix: string | null;
  onlyIfSmaller: boolean;
  /** SVG sources only: 1 is 100 % of their own size; `null` picks a sharp size automatically. */
  vectorScale: number | null;
};

export type ConvertOutcome = {
  converted: { source: string; destination: string }[];
  failed: { name: string; error: string }[];
  skipped: string[];
  cancelled: boolean;
};

export type ConvertCapabilities = { readableExtensions: string[]; writable: ImageFormat[] };

export type QuickKind = "png" | "jpeg" | "webp" | "optimize" | "png1x" | "png2x" | "png4x";

const FORMAT_BY_EXTENSION: Record<string, ImageFormat> = {
  png: "png",
  jpg: "jpeg",
  jpeg: "jpeg",
  webp: "webp",
  gif: "gif",
  bmp: "bmp",
  tif: "tiff",
  tiff: "tiff",
  ico: "ico",
  avif: "avif",
};

function extensionOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot < 0 ? "" : name.slice(dot + 1).toLowerCase();
}

export function imageFormatOf(name: string): ImageFormat | null {
  return FORMAT_BY_EXTENSION[extensionOf(name)] ?? null;
}

/** True when every name has an extension the backend can read. */
export function isConvertible(names: readonly string[], readable: readonly string[]): boolean {
  if (names.length === 0 || readable.length === 0) return false;
  return names.every((name) => readable.includes(extensionOf(name)));
}

const QUICK_TARGETS: readonly ImageFormat[] = ["png", "jpeg", "webp"];

/** Quick Convert entries to show: a target is hidden when every file already has that format. */
export function quickKindsFor(names: readonly string[]): QuickKind[] {
  // SVG files export like Figma: PNG at 1x, 2x or 4x of their own size.
  if (names.length > 0 && names.every((name) => extensionOf(name) === "svg")) {
    return ["png1x", "png2x", "png4x"];
  }
  const formats = names.map(imageFormatOf);
  const targets = QUICK_TARGETS.filter((target) => !formats.every((format) => format === target));
  // Optimize keeps each file's own format, so it needs one that can be written (not SVG).
  const canOptimize = formats.every((format) => format !== null);
  return [...(targets as QuickKind[]), ...(canOptimize ? (["optimize"] as const) : [])];
}

export function quickOptions(kind: QuickKind): ConvertOptions {
  const base: ConvertOptions = {
    format: null,
    quality: 85,
    resize: { kind: "none" },
    rotate: 0,
    flipH: false,
    flipV: false,
    stripMetadata: false,
    suffix: null,
    onlyIfSmaller: false,
    vectorScale: null,
  };
  if (kind === "optimize") {
    return { ...base, quality: 80, suffix: " (optimized)", onlyIfSmaller: true };
  }
  if (kind === "png1x" || kind === "png2x" || kind === "png4x") {
    return { ...base, format: "png", quality: 100, vectorScale: Number(kind[3]) };
  }
  return { ...base, format: kind };
}

/** Text of a Quick Convert entry. */
export function quickLabel(kind: QuickKind): string {
  if (kind === "optimize") return "Optimize";
  if (kind === "jpeg") return "JPG";
  if (kind === "png1x" || kind === "png2x" || kind === "png4x") return `PNG · ${kind[3]}×`;
  return kind.toUpperCase();
}

export function convertCapabilities(): Promise<ConvertCapabilities> {
  return invoke<ConvertCapabilities>("convert_capabilities");
}

export function estimateConvert(path: string, options: ConvertOptions): Promise<number> {
  return invoke<number>("estimate_convert", { path, options });
}

/** Progress and cancel go through the shared `transfer-progress` events (kind `convert`). */
export function convertImages(
  paths: string[],
  options: ConvertOptions,
  replaceOriginal: boolean,
): Promise<ConvertOutcome> {
  return invoke<ConvertOutcome>("convert_images", {
    jobId: crypto.randomUUID(),
    paths,
    options,
    replaceOriginal,
  });
}

/** Mirror of `ImageInfo` in `src-tauri/src/explorer/convert/mod.rs`. */
export type ImageInfo = {
  width: number;
  height: number;
  /** File size on disk. */
  bytes: number;
  hasGps: boolean;
  hasAlpha: boolean;
  animated: boolean;
};

export function convertImageInfo(path: string): Promise<ImageInfo> {
  return invoke<ImageInfo>("convert_image_info", { path });
}

/** The converted image at full resolution, as the exact bytes a conversion would write. */
export async function previewConvert(
  path: string,
  options: ConvertOptions,
  mime: string,
): Promise<Blob> {
  const buffer = await invoke<ArrayBuffer>("preview_convert", { path, options });
  return new Blob([buffer], { type: mime });
}
