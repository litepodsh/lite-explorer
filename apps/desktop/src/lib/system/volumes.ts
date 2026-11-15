import type { VolumeInfo } from "#lib/components/custom/overview/types.js";

/** The mounted volume that holds `path`: the one with the longest mount point that prefixes it. */
export function volumeForPath(volumes: readonly VolumeInfo[], path: string): VolumeInfo | null {
  if (!path) return null;
  const normalized = path.replace(/\\/g, "/").toLowerCase();
  let best: VolumeInfo | null = null;
  for (const volume of volumes) {
    const mount = volume.mount_point.replace(/\\/g, "/").replace(/\/$/, "").toLowerCase();
    const inside = mount === "" ? normalized.startsWith("/") : normalized === mount || normalized.startsWith(`${mount}/`);
    if (inside && (!best || volume.mount_point.length > best.mount_point.length)) best = volume;
  }
  return best;
}
