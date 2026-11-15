import { describe, expect, test } from "bun:test";
import { volumeForPath } from "./volumes.js";
import type { VolumeInfo } from "#lib/components/custom/overview/types.js";

const volume = (mount_point: string): VolumeInfo => ({
  name: mount_point,
  mount_point,
  file_system: "APFS",
  total_bytes: 100,
  free_bytes: 10,
  is_primary: mount_point === "/",
});

describe("volumeForPath", () => {
  const volumes = [volume("/"), volume("/Volumes/Backup"), volume("C:\\")];

  test("picks the deepest mount point that holds the path", () => {
    expect(volumeForPath(volumes, "/Users/me/Work")?.mount_point).toBe("/");
    expect(volumeForPath(volumes, "/Volumes/Backup/photos")?.mount_point).toBe("/Volumes/Backup");
    expect(volumeForPath(volumes, "/Volumes/Backup")?.mount_point).toBe("/Volumes/Backup");
  });

  test("does not match a sibling that only shares a prefix", () => {
    expect(volumeForPath(volumes, "/Volumes/Backups/x")?.mount_point).toBe("/");
  });

  test("matches Windows drives regardless of slash direction and case", () => {
    expect(volumeForPath(volumes, "c:/Users/me")?.mount_point).toBe("C:\\");
  });

  test("returns null for an empty path or a remote one", () => {
    expect(volumeForPath(volumes, "")).toBeNull();
    expect(volumeForPath([volume("/Volumes/Backup")], "s3://bucket/key")).toBeNull();
  });
});
