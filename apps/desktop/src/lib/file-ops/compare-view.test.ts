import { describe, expect, test } from "bun:test";
import {
  IDENTITY,
  MAX_SCALE,
  clampView,
  dividerAt,
  panBy,
  swapsAxes,
  zoomAt,
} from "./compare-view.js";

const W = 400;
const H = 300;

describe("zoomAt", () => {
  test("zooming in keeps the point under the cursor fixed", () => {
    const view = zoomAt(IDENTITY, 2, 100, 50, W, H);
    expect(view.scale).toBe(2);
    // The content point under the cursor (100, 50) must still be at (100, 50).
    expect(Math.abs((100 - view.x) / view.scale - 100) < 1e-6).toBe(true);
    expect(Math.abs((50 - view.y) / view.scale - 50) < 1e-6).toBe(true);
  });

  test("never zooms out past 1x or in past the maximum", () => {
    expect(zoomAt(IDENTITY, 0.1, 10, 10, W, H)).toEqual(IDENTITY);
    expect(zoomAt(IDENTITY, 100, 200, 150, W, H).scale).toBe(MAX_SCALE);
  });

  test("zooming back out to 1x returns to the origin", () => {
    const zoomed = zoomAt(IDENTITY, 4, 300, 200, W, H);
    const back = zoomAt(zoomed, 0.25, 300, 200, W, H);
    expect(back).toEqual(IDENTITY);
  });
});

describe("clampView / panBy", () => {
  test("panning is limited so the content always covers the stage", () => {
    const zoomed = { scale: 2, x: 0, y: 0 };
    expect(panBy(zoomed, 50, 50, W, H)).toEqual({ scale: 2, x: 0, y: 0 });
    expect(panBy(zoomed, -1000, -1000, W, H)).toEqual({ scale: 2, x: -W, y: -H });
  });

  test("a 1x view cannot move", () => {
    expect(panBy(IDENTITY, 30, -30, W, H)).toEqual(IDENTITY);
  });

  test("clampView also clamps the scale", () => {
    expect(clampView({ scale: 20, x: 0, y: 0 }, W, H).scale).toBe(MAX_SCALE);
  });
});

describe("dividerAt", () => {
  test("maps the pointer to a percentage of the stage", () => {
    expect(dividerAt(300, 100, 400)).toBe(50);
  });
  test("stays inside the stage", () => {
    expect(dividerAt(0, 100, 400)).toBe(0);
    expect(dividerAt(900, 100, 400)).toBe(100);
  });
  test("an empty stage centers the divider", () => {
    expect(dividerAt(10, 0, 0)).toBe(50);
  });
});

describe("swapsAxes", () => {
  test("only quarter turns swap", () => {
    expect(swapsAxes(0)).toBe(false);
    expect(swapsAxes(90)).toBe(true);
    expect(swapsAxes(180)).toBe(false);
    expect(swapsAxes(270)).toBe(true);
  });
});
