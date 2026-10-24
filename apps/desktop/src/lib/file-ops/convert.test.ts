import { describe, expect, test } from "bun:test";
import {
  imageFormatOf,
  isConvertible,
  quickKindsFor,
  quickLabel,
  quickOptions,
} from "./convert.js";

const READABLE = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "ico"];

describe("imageFormatOf", () => {
  test("maps extensions case-insensitively", () => {
    expect(imageFormatOf("Photo.JPG")).toBe("jpeg");
    expect(imageFormatOf("a.tif")).toBe("tiff");
    expect(imageFormatOf("a.png")).toBe("png");
  });

  test("knows AVIF but not SVG, which can only be read", () => {
    expect(imageFormatOf("a.avif")).toBe("avif");
    expect(imageFormatOf("logo.svg")).toBeNull();
  });

  test("returns null for non-images and names without extension", () => {
    expect(imageFormatOf("notes.txt")).toBeNull();
    expect(imageFormatOf("README")).toBeNull();
  });
});

describe("isConvertible", () => {
  test("true only when every name is a readable image", () => {
    expect(isConvertible(["a.png", "b.JPG"], READABLE)).toBe(true);
    expect(isConvertible(["a.png", "b.txt"], READABLE)).toBe(false);
  });

  test("false for an empty selection or an empty capability list", () => {
    expect(isConvertible([], READABLE)).toBe(false);
    expect(isConvertible(["a.png"], [])).toBe(false);
  });
});

describe("quickKindsFor", () => {
  test("hides a target when every file is already that format", () => {
    expect(quickKindsFor(["a.png"])).toEqual(["jpeg", "webp", "optimize"]);
    expect(quickKindsFor(["a.jpg", "b.jpeg"])).toEqual(["png", "webp", "optimize"]);
  });

  test("SVG files export like Figma: PNG at 1x, 2x and 4x", () => {
    expect(quickKindsFor(["logo.svg"])).toEqual(["png1x", "png2x", "png4x"]);
    expect(quickKindsFor(["a.svg", "B.SVG"])).toEqual(["png1x", "png2x", "png4x"]);
  });

  test("does not offer Optimize when a file cannot be written back (SVG mixed with rasters)", () => {
    expect(quickKindsFor(["a.png", "logo.svg"])).toEqual(["png", "jpeg", "webp"]);
  });

  test("offers every target for mixed selections", () => {
    expect(quickKindsFor(["a.png", "b.jpg"])).toEqual(["png", "jpeg", "webp", "optimize"]);
  });
});

describe("quickOptions", () => {
  test("converts to the requested format at quality 85", () => {
    const options = quickOptions("jpeg");
    expect(options.format).toBe("jpeg");
    expect(options.quality).toBe(85);
    expect(options.suffix).toBeNull();
    expect(options.onlyIfSmaller).toBe(false);
  });

  test("optimize keeps each format, uses quality 80 and only saves smaller files", () => {
    const options = quickOptions("optimize");
    expect(options.format).toBeNull();
    expect(options.quality).toBe(80);
    expect(options.suffix).toBe(" (optimized)");
    expect(options.onlyIfSmaller).toBe(true);
  });

  test("SVG exports are lossless PNG at the chosen scale", () => {
    for (const [kind, scale] of [["png1x", 1], ["png2x", 2], ["png4x", 4]] as const) {
      const options = quickOptions(kind);
      expect(options.format).toBe("png");
      expect(options.vectorScale).toBe(scale);
      expect(options.quality).toBe(100);
    }
  });

  test("other quick kinds let the backend pick a sharp SVG size", () => {
    expect(quickOptions("jpeg").vectorScale).toBeNull();
    expect(quickOptions("optimize").vectorScale).toBeNull();
  });

  test("never resizes, rotates or flips", () => {
    for (const kind of ["png", "jpeg", "webp", "optimize", "png1x", "png2x", "png4x"] as const) {
      const options = quickOptions(kind);
      expect(options.resize).toEqual({ kind: "none" });
      expect(options.rotate).toBe(0);
      expect(options.flipH).toBe(false);
      expect(options.flipV).toBe(false);
    }
  });
});

describe("quickLabel", () => {
  test("reads well in the menu", () => {
    expect(quickLabel("jpeg")).toBe("JPG");
    expect(quickLabel("webp")).toBe("WEBP");
    expect(quickLabel("optimize")).toBe("Optimize");
    expect(quickLabel("png2x")).toBe("PNG · 2×");
  });
});
