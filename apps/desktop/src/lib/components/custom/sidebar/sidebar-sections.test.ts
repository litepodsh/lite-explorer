import { describe, expect, test } from "bun:test";
import { sidebarLocations, volumeLocations } from "./sidebar-sections.js";

describe("sidebar sections", () => {
  const locations = [
    { name: "Macintosh HD", path: "/", kind: "volume" },
    { name: "Archive", path: "/Volumes/Archive", kind: "hfs-volume" },
    { name: "sebastian", path: "/Users/sebastian", kind: "home" },
    { name: "Drive", path: "s3://drive/", kind: "s3" },
  ];

  test("keeps only OS volumes in Volumes", () => {
    expect(volumeLocations(locations)).toEqual(locations.slice(0, 2));
    expect(sidebarLocations(locations)).toEqual(locations.slice(2));
  });
});
