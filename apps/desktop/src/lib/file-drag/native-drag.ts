import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { isNetworkPath } from "$lib/remote/network-locations.js";
import { isRemotePath } from "$lib/remote/remote-locations.js";

export type DragIcon = "folder" | "file" | "drive";

/** Only local items have a file other apps can open; buckets, shares and remote paths don't. */
export function canDragOut(entry: { path: string; kind?: string }): boolean {
  return !entry.kind && !isRemotePath(entry.path) && !isNetworkPath(entry.path);
}

/** True when the pointer has reached the window edge, where an internal drag hands off to the OS. */
export function atWindowEdge(x: number, y: number, width: number, height: number): boolean {
  return x <= 0 || y <= 0 || x >= width - 1 || y >= height - 1;
}

const CRC_TABLE = Array.from({ length: 256 }, (_, index) => {
  let crc = index;
  for (let bit = 0; bit < 8; bit++) crc = crc & 1 ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1;
  return crc >>> 0;
});

export function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** PNG signature (8 bytes) plus the IHDR chunk (25 bytes). */
const IHDR_END = 33;

/**
 * Adds a pHYs chunk so the PNG reads as `scale` times 72 DPI. macOS sizes drag images by DPI,
 * so an image drawn at 2x for a Retina screen shows at its intended size instead of doubled.
 */
export function pngWithDensity(png: Uint8Array, scale: number): Uint8Array {
  const pixelsPerMeter = Math.round((72 * scale) / 0.0254);
  const chunk = new Uint8Array(21);
  const view = new DataView(chunk.buffer);
  view.setUint32(0, 9);
  chunk.set([0x70, 0x48, 0x59, 0x73], 4); // "pHYs"
  view.setUint32(8, pixelsPerMeter);
  view.setUint32(12, pixelsPerMeter);
  chunk[16] = 1; // unit: meter
  view.setUint32(17, crc32(chunk.subarray(4, 17)));
  const result = new Uint8Array(png.length + chunk.length);
  result.set(png.subarray(0, IHDR_END));
  result.set(chunk, IHDR_END);
  result.set(png.subarray(IHDR_END), IHDR_END + chunk.length);
  return result;
}

// Lucide icon paths, drawn on a 24px grid.
const ICON_PATHS: Record<DragIcon, string[]> = {
  folder: [
    "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
  ],
  file: [
    "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
    "M14 2v5a1 1 0 0 0 1 1h5",
  ],
  drive: [
    "M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z",
    "M21.946 12.013H2.054",
  ],
};

/** Draws the drag card (the same look as the in-app drag ghost) as a PNG data URL. */
function dragCardImage(name: string, icon: DragIcon, count = 1): string {
  const scale = Math.max(1, Math.round(window.devicePixelRatio || 1));
  const font = `13px ${getComputedStyle(document.body).fontFamily}`;
  const shadow = 14;
  const height = 34;
  const measure = document.createElement("canvas").getContext("2d")!;
  measure.font = font;
  const label = fitText(measure, name, 220);
  const width = Math.ceil(9 + 16 + 8 + measure.measureText(label).width + 12);

  const canvas = document.createElement("canvas");
  canvas.width = (width + shadow * 2) * scale;
  canvas.height = (height + shadow * 2) * scale;
  const context = canvas.getContext("2d")!;
  context.scale(scale, scale);
  context.translate(shadow, shadow - 4);

  // Several items: two cards peek out behind the front one.
  for (const [offset, alpha] of count > 1
    ? ([
        [6, 0.35],
        [3, 0.6],
      ] as const)
    : []) {
    context.fillStyle = `rgb(44 41 39 / ${alpha})`;
    context.beginPath();
    context.roundRect(offset, offset, width, height, 10);
    context.fill();
  }

  context.save();
  context.shadowColor = "rgb(0 0 0 / 0.45)";
  context.shadowBlur = 12;
  context.shadowOffsetY = 5;
  context.fillStyle = "rgb(44 41 39 / 0.96)";
  context.beginPath();
  context.roundRect(0, 0, width, height, 10);
  context.fill();
  context.restore();
  context.strokeStyle = "rgb(255 255 255 / 0.1)";
  context.lineWidth = 1;
  context.beginPath();
  context.roundRect(0.5, 0.5, width - 1, height - 1, 9.5);
  context.stroke();

  context.save();
  context.translate(9, (height - 16) / 2);
  context.scale(16 / 24, 16 / 24);
  context.strokeStyle = icon === "file" ? "#aaa5a1" : "#0a9bff";
  context.lineWidth = 1.8 * (24 / 16);
  context.lineCap = "round";
  context.lineJoin = "round";
  for (const path of ICON_PATHS[icon]) context.stroke(new Path2D(path));
  context.restore();

  context.font = font;
  context.fillStyle = "#eceae8";
  context.textBaseline = "middle";
  context.fillText(label, 9 + 16 + 8, height / 2 + 0.5);

  const png = Uint8Array.from(atob(canvas.toDataURL("image/png").split(",")[1]), (character) =>
    character.charCodeAt(0),
  );
  let binary = "";
  for (const byte of pngWithDensity(png, scale)) binary += String.fromCharCode(byte);
  return `data:image/png;base64,${btoa(binary)}`;
}

function fitText(context: CanvasRenderingContext2D, text: string, maxWidth: number): string {
  if (context.measureText(text).width <= maxWidth) return text;
  let end = text.length;
  while (end > 1 && context.measureText(`${text.slice(0, end)}…`).width > maxWidth) end--;
  return `${text.slice(0, end)}…`;
}

/** Hands a drag of local files or folders to the OS, so they can be dropped in other apps. */
export function startNativeDrag(paths: string[], name: string, icon: DragIcon) {
  const label = paths.length > 1 ? `${paths.length} items` : name;
  startDrag({ item: paths, icon: dragCardImage(label, icon, paths.length), mode: "copy" }).catch(
    (error) => console.error("Couldn't start dragging out of the app", error),
  );
}
