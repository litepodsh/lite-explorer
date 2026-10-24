import { describe, expect, test } from "bun:test";
import type { ImageInfo } from "./convert.js";
import {
  PERCENT_CHOICES,
  SIZE_PRESETS,
  VECTOR_SCALES,
  canPreview,
  createLatest,
  defaultState,
  formatBytes,
  hasQuality,
  hasRaster,
  hasVector,
  isValid,
  mimeType,
  percentChange,
  rotateBy,
  scaleLabel,
  sizeLine,
  toOptions,
  vectorOutputSize,
  warningsFor,
} from "./convert-dialog-model.js";

const ALL = ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico", "avif"] as const;

const info = (overrides: Partial<ImageInfo> = {}): ImageInfo => ({
  width: 800,
  height: 600,
  bytes: 1000,
  hasGps: false,
  hasAlpha: false,
  animated: false,
  ...overrides,
});

describe("defaultState", () => {
  test("picks WebP for PNG and JPG sources", () => {
    expect(defaultState(["a.png"], ALL).format).toBe("webp");
    expect(defaultState(["a.jpg"], ALL).format).toBe("webp");
  });

  test("picks JPEG for WebP sources", () => {
    expect(defaultState(["a.webp"], ALL).format).toBe("jpeg");
  });

  test("rasterizes SVG to PNG by default", () => {
    expect(defaultState(["logo.svg"], ALL).format).toBe("png");
    expect(defaultState(["logo.SVG"], ["jpeg", "webp"]).format).toBe("webp");
  });

  test("SVG starts at 100 % size and the best quality", () => {
    const state = defaultState(["logo.svg"], ALL);
    expect(state.vectorScale).toBe(1);
    expect(state.quality).toBe(100);
    expect(defaultState(["photo.png"], ALL).quality).toBe(85);
  });

  test("falls back to the first writable format", () => {
    expect(defaultState(["a.png"], ["png", "jpeg"]).format).toBe("jpeg");
    expect(defaultState(["a.png"], ["png"]).format).toBe("png");
  });

  test("starts neutral: original size, no rotation, metadata kept, original kept", () => {
    const state = defaultState(["a.png"], ALL);
    expect(state.sizeMode).toBe("original");
    expect(state.rotate).toBe(0);
    expect(state.flipH || state.flipV).toBe(false);
    expect(state.stripMetadata).toBe(false);
    expect(state.replaceOriginal).toBe(false);
    expect(state.quality).toBe(85);
  });
});

describe("toOptions", () => {
  test("maps size modes to Resize", () => {
    const base = defaultState(["a.png"], ALL);
    expect(toOptions(base).resize).toEqual({ kind: "none" });
    expect(toOptions({ ...base, sizeMode: "percent", percent: 50 }).resize).toEqual({
      kind: "percent",
      value: 50,
    });
    expect(
      toOptions({ ...base, sizeMode: "fit", fitWidth: 1080, fitHeight: 0 }).resize,
    ).toEqual({ kind: "fit", width: 1080, height: 0 });
  });

  test("passes format, quality, transforms and metadata through", () => {
    const state = {
      ...defaultState(["a.png"], ALL),
      format: "jpeg" as const,
      quality: 60,
      rotate: 90 as const,
      flipH: true,
      stripMetadata: true,
    };
    const options = toOptions(state);
    expect(options.format).toBe("jpeg");
    expect(options.quality).toBe(60);
    expect(options.rotate).toBe(90);
    expect(options.flipH).toBe(true);
    expect(options.stripMetadata).toBe(true);
    expect(options.suffix).toBeNull();
    expect(options.onlyIfSmaller).toBe(false);
  });

  test("sends the SVG scale", () => {
    const state = { ...defaultState(["logo.svg"], ALL), vectorScale: 4 };
    expect(toOptions(state).vectorScale).toBe(4);
  });
});

describe("isValid", () => {
  const base = defaultState(["a.png"], ALL);
  test("original size is always valid", () => {
    expect(isValid(base)).toBe(true);
  });
  test("percent must be 1 to 100", () => {
    expect(isValid({ ...base, sizeMode: "percent", percent: 0 })).toBe(false);
    expect(isValid({ ...base, sizeMode: "percent", percent: 101 })).toBe(false);
    expect(isValid({ ...base, sizeMode: "percent", percent: 50 })).toBe(true);
  });
  test("the SVG scale must be a sane positive number", () => {
    expect(isValid({ ...base, vectorScale: 0 })).toBe(false);
    expect(isValid({ ...base, vectorScale: Number.NaN })).toBe(false);
    expect(isValid({ ...base, vectorScale: 65 })).toBe(false);
    expect(isValid({ ...base, vectorScale: 0.5 })).toBe(true);
    expect(isValid({ ...base, vectorScale: 64 })).toBe(true);
  });
  test("fit needs at least one positive side", () => {
    expect(isValid({ ...base, sizeMode: "fit", fitWidth: 0, fitHeight: 0 })).toBe(false);
    expect(isValid({ ...base, sizeMode: "fit", fitWidth: 0, fitHeight: 300 })).toBe(true);
    expect(isValid({ ...base, sizeMode: "fit", fitWidth: -5, fitHeight: 0 })).toBe(false);
  });
});

describe("rotateBy", () => {
  test("wraps around in both directions", () => {
    expect(rotateBy(0, 90)).toBe(90);
    expect(rotateBy(270, 90)).toBe(0);
    expect(rotateBy(0, -90)).toBe(270);
    expect(rotateBy(90, -90)).toBe(0);
  });
});

describe("hasQuality", () => {
  test("only lossy formats", () => {
    expect(hasQuality("jpeg")).toBe(true);
    expect(hasQuality("webp")).toBe(true);
    expect(hasQuality("avif")).toBe(true);
    expect(hasQuality("png")).toBe(false);
  });
});

describe("sizes", () => {
  test("formatBytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(380 * 1024)).toBe("380 KB");
    expect(formatBytes(2.4 * 1024 * 1024)).toBe("2.4 MB");
  });

  test("sizeLine shows the change", () => {
    expect(sizeLine(2.4 * 1024 * 1024, 380 * 1024)).toBe("2.4 MB → 380 KB (−85%)");
    expect(sizeLine(1000, 1100)).toBe("1000 B → 1 KB (+10%)");
  });

  test("sizeLine with an empty original omits the percentage", () => {
    expect(sizeLine(0, 2048)).toBe("0 B → 2 KB");
  });
});

describe("presets", () => {
  test("are sane", () => {
    expect(PERCENT_CHOICES).toEqual([25, 50, 75]);
    expect(SIZE_PRESETS.map((preset) => preset.width)).toEqual([256, 1080, 2048]);
  });
});

describe("warningsFor", () => {
  const base = defaultState(["a.png"], ALL);

  test("no info, no warnings", () => {
    expect(warningsFor(base, null)).toEqual([]);
  });

  test("transparency to JPEG or BMP", () => {
    const transparent = info({ hasAlpha: true });
    expect(warningsFor({ ...base, format: "jpeg" }, transparent)).toContain(
      "Transparency becomes a white background.",
    );
    expect(warningsFor({ ...base, format: "bmp" }, transparent).length).toBe(1);
    expect(warningsFor({ ...base, format: "webp" }, transparent)).toEqual([]);
  });

  test("animated GIF to a non-GIF format", () => {
    const animated = info({ animated: true });
    expect(warningsFor({ ...base, format: "png" }, animated)).toContain(
      "Only the first frame is kept.",
    );
    expect(warningsFor({ ...base, format: "gif" }, animated)).toEqual([]);
  });

  test("ICO limit", () => {
    expect(warningsFor({ ...base, format: "ico" }, info({ width: 512 }))).toContain(
      "ICO is limited to 256 px; the image will be reduced.",
    );
    expect(warningsFor({ ...base, format: "ico" }, info({ width: 200, height: 200 }))).toEqual([]);
  });

  test("location is mentioned only while metadata is kept", () => {
    const located = info({ hasGps: true });
    expect(warningsFor(base, located)).toContain(
      "This image contains a location. Turn on “Remove EXIF and location” to delete it.",
    );
    expect(warningsFor({ ...base, stripMetadata: true }, located)).toEqual([]);
  });
});

describe("createLatest", () => {
  test("only the newest token is current", () => {
    const latest = createLatest();
    const first = latest.next();
    const second = latest.next();
    expect(latest.isCurrent(first)).toBe(false);
    expect(latest.isCurrent(second)).toBe(true);
  });
});

describe("percentChange", () => {
  test("signs and rounding", () => {
    expect(percentChange(1000, 150)).toBe("−85%");
    expect(percentChange(1000, 1100)).toBe("+10%");
    expect(percentChange(1000, 1000)).toBe("+0%");
  });
  test("empty without an original size", () => {
    expect(percentChange(0, 500)).toBe("");
  });
});

describe("mimeType / canPreview", () => {
  test("every format has an image mime type", () => {
    for (const format of ["png", "jpeg", "webp", "gif", "bmp", "tiff", "ico", "avif"] as const) {
      expect(mimeType(format).startsWith("image/")).toBe(true);
    }
    expect(mimeType("jpeg")).toBe("image/jpeg");
    expect(mimeType("avif")).toBe("image/avif");
  });
  test("TIFF cannot be previewed", () => {
    expect(canPreview("tiff")).toBe(false);
    expect(canPreview("webp")).toBe(true);
  });
});

describe("vector helpers", () => {
  test("detect SVG names in a selection", () => {
    expect(hasVector(["a.png", "logo.SVG"])).toBe(true);
    expect(hasVector(["a.png"])).toBe(false);
    expect(hasRaster(["a.svg"])).toBe(false);
    expect(hasRaster(["a.svg", "b.png"])).toBe(true);
  });

  test("export scales are Figma's", () => {
    expect([...VECTOR_SCALES]).toEqual([0.5, 1, 2, 3, 4]);
    expect(scaleLabel(1)).toBe("1×");
    expect(scaleLabel(0.5)).toBe("0.5×");
    expect(scaleLabel(2.25)).toBe("2.25×");
  });

  test("output size follows the scale and mirrors the backend", () => {
    expect(vectorOutputSize(64, 32, 1)).toEqual({ width: 64, height: 32 });
    expect(vectorOutputSize(64, 32, 4)).toEqual({ width: 256, height: 128 });
    expect(vectorOutputSize(64, 32, 0.5)).toEqual({ width: 32, height: 16 });
    expect(vectorOutputSize(1000, 500, 64)).toEqual({ width: 8192, height: 4096 });
  });
});
