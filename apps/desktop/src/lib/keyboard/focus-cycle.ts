import type { Region } from "./scope.js";

export type RegionSlot = { region: Region; paneId: string | null };

function sameSlot(a: RegionSlot, b: RegionSlot): boolean {
  return a.region === b.region && a.paneId === b.paneId;
}

/** The region `F6` (1) or `Shift+F6` (-1) moves to. */
export function nextRegion(
  slots: RegionSlot[],
  current: RegionSlot | null,
  direction: 1 | -1,
): RegionSlot | null {
  if (slots.length === 0) return null;
  const index = current ? slots.findIndex((slot) => sameSlot(slot, current)) : -1;
  if (index < 0) return direction === 1 ? slots[0] : slots[slots.length - 1];
  return slots[(index + direction + slots.length) % slots.length];
}
