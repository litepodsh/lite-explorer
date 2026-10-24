import {
  imageFormatOf,
  type ConvertOptions,
  type ImageFormat,
  type ImageInfo,
  type Resize,
} from "./convert.js";

export type SizeMode = "original" | "percent" | "fit";
type Rotation = 0 | 90 | 180 | 270;

export type DialogState = {
  format: ImageFormat;
  quality: number;
  sizeMode: SizeMode;
  percent: number;
  fitWidth: number;
  fitHeight: number;
  rotate: Rotation;
  flipH: boolean;
  flipV: boolean;
  stripMetadata: boolean;
  replaceOriginal: boolean;
  /** SVG sources: 1 is 100 % of their own size. */
  vectorScale: number;
};

/** Export scales offered for SVG, like Figma's. */
export const VECTOR_SCALES = [0.5, 1, 2, 3, 4] as const;

const VECTOR_MAX_SIDE = 8192;
const VECTOR_MIN_SCALE = 0.01;
const VECTOR_MAX_SCALE = 64;

export function isVectorName(name: string): boolean {
  return /\.svg$/i.test(name);
}

export function hasVector(names: readonly string[]): boolean {
  return names.some(isVectorName);
}

export function hasRaster(names: readonly string[]): boolean {
  return names.some((name) => !isVectorName(name));
}

/** "2×", "0.5×". */
export function scaleLabel(scale: number): string {
  return `${Number.isInteger(scale) ? scale : scale.toFixed(2).replace(/0+$/, "")}×`;
}

/** Size an SVG of the given own size is exported at; mirrors `svg::render_size` in Rust. */
export function vectorOutputSize(
  width: number,
  height: number,
  scale: number,
): { width: number; height: number } {
  const longest = Math.max(width, height, 1);
  let factor = Math.min(Math.max(scale, VECTOR_MIN_SCALE), VECTOR_MAX_SCALE);
  if (longest * factor > VECTOR_MAX_SIDE) factor = VECTOR_MAX_SIDE / longest;
  return {
    width: Math.max(1, Math.round(width * factor)),
    height: Math.max(1, Math.round(height * factor)),
  };
}

export const PERCENT_CHOICES = [25, 50, 75] as const;

export const SIZE_PRESETS = [
  { label: "Thumbnail 256", width: 256, height: 256 },
  { label: "Web 1080", width: 1080, height: 1080 },
  { label: "2048", width: 2048, height: 2048 },
] as const;

const LABELS: Record<ImageFormat, string> = {
  png: "PNG",
  jpeg: "JPG",
  webp: "WebP",
  gif: "GIF",
  bmp: "BMP",
  tiff: "TIFF",
  ico: "ICO",
  avif: "AVIF",
};

const PREFERRED: readonly ImageFormat[] = ["webp", "jpeg", "png"];

export function formatLabel(format: ImageFormat): string {
  return LABELS[format];
}

export function hasQuality(format: ImageFormat): boolean {
  return format === "jpeg" || format === "webp" || format === "avif";
}

/** First preferred format that is writable and differs from the first file's own format. */
export function defaultState(
  names: readonly string[],
  writable: readonly ImageFormat[],
): DialogState {
  const source = names.length > 0 ? imageFormatOf(names[0]) : null;
  // A vector source has no raster format of its own: PNG keeps it lossless.
  const vector = names.length > 0 && isVectorName(names[0]);
  const format = vector && writable.includes("png")
    ? "png"
    : (PREFERRED.find((candidate) => candidate !== source && writable.includes(candidate)) ??
      writable[0] ??
      "png");
  return {
    format,
    // Vector art is exported at the best quality: 100 is lossless in WebP.
    quality: vector ? 100 : 85,
    sizeMode: "original",
    percent: 50,
    fitWidth: 1080,
    fitHeight: 1080,
    rotate: 0,
    flipH: false,
    flipV: false,
    stripMetadata: false,
    replaceOriginal: false,
    vectorScale: 1,
  };
}

function resizeOf(state: DialogState): Resize {
  switch (state.sizeMode) {
    case "percent":
      return { kind: "percent", value: state.percent };
    case "fit":
      return { kind: "fit", width: state.fitWidth, height: state.fitHeight };
    default:
      return { kind: "none" };
  }
}

export function toOptions(state: DialogState): ConvertOptions {
  return {
    format: state.format,
    quality: state.quality,
    resize: resizeOf(state),
    rotate: state.rotate,
    flipH: state.flipH,
    flipV: state.flipV,
    stripMetadata: state.stripMetadata,
    suffix: null,
    onlyIfSmaller: false,
    vectorScale: state.vectorScale,
  };
}

export function isValid(state: DialogState): boolean {
  if (
    !Number.isFinite(state.vectorScale) ||
    state.vectorScale < VECTOR_MIN_SCALE ||
    state.vectorScale > VECTOR_MAX_SCALE
  ) {
    return false;
  }
  if (state.sizeMode === "percent") {
    return Number.isInteger(state.percent) && state.percent >= 1 && state.percent <= 100;
  }
  if (state.sizeMode === "fit") {
    const { fitWidth, fitHeight } = state;
    if (fitWidth < 0 || fitHeight < 0) return false;
    return fitWidth > 0 || fitHeight > 0;
  }
  return true;
}

export function rotateBy(current: Rotation, delta: 90 | -90): Rotation {
  return ((((current + delta) % 360) + 360) % 360) as Rotation;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "−85%" or "+10%". Empty when the original has no size. */
export function percentChange(before: number, after: number): string {
  if (before <= 0) return "";
  const change = Math.round(((after - before) / before) * 100);
  return `${change < 0 ? "−" : "+"}${Math.abs(change)}%`;
}

/** "2.4 MB → 380 KB (−85%)". The percentage is left out when the original is empty. */
export function sizeLine(before: number, after: number): string {
  const line = `${formatBytes(before)} → ${formatBytes(after)}`;
  const change = percentChange(before, after);
  return change ? `${line} (${change})` : line;
}

const MIME: Record<ImageFormat, string> = {
  png: "image/png",
  jpeg: "image/jpeg",
  webp: "image/webp",
  gif: "image/gif",
  bmp: "image/bmp",
  tiff: "image/tiff",
  ico: "image/x-icon",
  avif: "image/avif",
};

export function mimeType(format: ImageFormat): string {
  return MIME[format];
}

/** TIFF cannot be shown by the system web views on Windows and Linux. */
export function canPreview(format: ImageFormat): boolean {
  return format !== "tiff";
}

export function warningsFor(state: DialogState, info: ImageInfo | null): string[] {
  if (!info) return [];
  const warnings: string[] = [];
  if (info.hasAlpha && (state.format === "jpeg" || state.format === "bmp")) {
    warnings.push("Transparency becomes a white background.");
  }
  if (info.animated && state.format !== "gif") {
    warnings.push("Only the first frame is kept.");
  }
  if (state.format === "ico" && (info.width > 256 || info.height > 256)) {
    warnings.push("ICO is limited to 256 px; the image will be reduced.");
  }
  if (info.hasGps && !state.stripMetadata) {
    warnings.push(
      "This image contains a location. Turn on “Remove EXIF and location” to delete it.",
    );
  }
  return warnings;
}

/** Hands out tokens so only the newest async response is applied. */
export function createLatest() {
  let current = 0;
  return {
    next: () => ++current,
    isCurrent: (token: number) => token === current,
  };
}
