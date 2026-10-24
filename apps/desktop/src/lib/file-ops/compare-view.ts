/** Zoom and pan of the before/after viewer. Both layers share one transform. */
export type View = { scale: number; x: number; y: number };

export const MIN_SCALE = 1;
export const MAX_SCALE = 8;

export const IDENTITY: View = { scale: 1, x: 0, y: 0 };

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Keeps the zoomed content covering the stage: no empty borders while zoomed in. */
export function clampView(view: View, width: number, height: number): View {
  const scale = clamp(view.scale, MIN_SCALE, MAX_SCALE);
  return {
    scale,
    x: clamp(view.x, width - width * scale, 0),
    y: clamp(view.y, height - height * scale, 0),
  };
}

/** Multiplies the scale by `factor`, keeping the point (cx, cy) of the stage fixed. */
export function zoomAt(
  view: View,
  factor: number,
  cx: number,
  cy: number,
  width: number,
  height: number,
): View {
  const scale = clamp(view.scale * factor, MIN_SCALE, MAX_SCALE);
  const ratio = scale / view.scale;
  return clampView(
    { scale, x: cx - (cx - view.x) * ratio, y: cy - (cy - view.y) * ratio },
    width,
    height,
  );
}

export function panBy(view: View, dx: number, dy: number, width: number, height: number): View {
  return clampView({ ...view, x: view.x + dx, y: view.y + dy }, width, height);
}

/** Divider position as a percentage of the stage width, kept inside 0 to 100. */
export function dividerAt(clientX: number, left: number, width: number): number {
  if (width <= 0) return 50;
  return clamp(((clientX - left) / width) * 100, 0, 100);
}

/** True when the rotation swaps width and height. */
export function swapsAxes(rotate: number): boolean {
  return rotate % 180 !== 0;
}
