export const DEFAULT_FONT_SIZE = 12;
const MIN_FONT_SIZE = 8;
const MAX_FONT_SIZE = 24;

export function clampFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_FONT_SIZE;
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(size)));
}

export function parseFontSize(saved: string | null): number {
  if (!saved) return DEFAULT_FONT_SIZE;
  return clampFontSize(Number(saved));
}

export function fontSizeForShortcut(current: number, key: string): number | null {
  if (key === "-") return clampFontSize(current - 1);
  if (key === "=" || key === "+") return clampFontSize(current + 1);
  if (key === "0") return DEFAULT_FONT_SIZE;
  return null;
}

export function lineHeightFor(size: number): number {
  return Math.round(size * 1.5);
}
