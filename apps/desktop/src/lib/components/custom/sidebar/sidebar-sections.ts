import type { Location } from "$lib/tabs/tabs.js";

const VOLUME_KINDS = new Set(["volume", "hfs-volume", "wsl-volume"]);

export function isVolumeLocation({ kind }: Location): boolean {
  return VOLUME_KINDS.has(kind);
}

export function volumeLocations(locations: Location[]): Location[] {
  return locations.filter(isVolumeLocation);
}

export function sidebarLocations(locations: Location[]): Location[] {
  return locations.filter((location) => !isVolumeLocation(location));
}
