import type { Location } from "$lib/tabs/tabs.js";

export function volumeLocations(locations: Location[]): Location[] {
  return locations.filter(({ kind }) => kind === "volume" || kind === "hfs-volume");
}

export function sidebarLocations(locations: Location[]): Location[] {
  return locations.filter(({ kind }) => kind !== "volume" && kind !== "hfs-volume");
}
