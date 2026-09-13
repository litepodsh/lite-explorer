import { describe, expect, test } from "bun:test";
import { emptyNetworkInput } from "./network-locations.js";
import { displayPath, findOwner, remapMountPath, type KnownLocation } from "./network-paths.js";

const photos: KnownLocation = {
  location: { name: "Photos", path: "smb://a1/Photos", kind: "smb" },
  mountPath: "/Volumes/Photos",
  input: { ...emptyNetworkInput("smb"), host: "nas.local", path: "Photos" },
};
const archive: KnownLocation = {
  location: { name: "Archive", path: "smb://b2/Photos/archive", kind: "smb" },
  mountPath: "/Volumes/Photos/archive",
  input: { ...emptyNetworkInput("smb"), host: "nas.local", path: "Photos/archive" },
};
const pi: KnownLocation = {
  location: { name: "Pi", path: "sftp://c3/home/pi", kind: "sftp" },
  input: {
    ...emptyNetworkInput("sftp"),
    host: "raspberry.local",
    username: "pi",
    path: "/home/pi",
  },
};

const server: KnownLocation = {
  location: { name: "PC", path: "smb://d4/", kind: "smb" },
  mountPath: "smb://d4/",
  shares: [{ name: "Users", path: "/Volumes/Users" }],
  input: { ...emptyNetworkInput("smb"), host: "192.168.1.65", username: "sebas", auth: "password" },
};

describe("network paths", () => {
  test("a server without a share owns its listing and mounted shares", () => {
    const known = [photos, server];
    expect(findOwner("smb://d4/", known)?.location.name).toBe("PC");
    expect(findOwner("smb://d4/smbtest", known)?.location.name).toBe("PC");
    expect(findOwner("/Volumes/Users/sebas", known)?.location.name).toBe("PC");
    expect(displayPath("smb://d4/", server)).toBe("smb://sebas@192.168.1.65");
    expect(displayPath("/Volumes/Users/sebas", server)).toBe(
      "smb://sebas@192.168.1.65/Users/sebas",
    );
  });

  test("owners match server ids and the deepest mount", () => {
    const known = [photos, archive, pi];
    expect(findOwner("sftp://c3/etc", known)?.location.name).toBe("Pi");
    expect(findOwner("sftp://zz/etc", known)).toBeNull();
    expect(findOwner("/Volumes/Photos/2026", known)?.location.name).toBe("Photos");
    expect(findOwner("/Volumes/Photos/archive/old", known)?.location.name).toBe("Archive");
    expect(findOwner("/Volumes/Photos Old", known)).toBeNull();
    expect(findOwner("/Users/me", known)).toBeNull();
  });

  test("addresses replace internal ids and mount folders", () => {
    expect(displayPath("sftp://c3/home/pi/notes", pi)).toBe(
      "sftp://pi@raspberry.local/home/pi/notes",
    );
    expect(displayPath("sftp://c3/", pi)).toBe("sftp://pi@raspberry.local/");
    expect(displayPath("/Volumes/Photos", photos)).toBe("smb://nas.local/Photos");
    expect(displayPath("/Volumes/Photos/2026/june", photos)).toBe(
      "smb://nas.local/Photos/2026/june",
    );
    expect(
      displayPath("\\\\nas.local\\Photos\\2026", { ...photos, mountPath: "\\\\nas.local\\Photos" }),
    ).toBe("smb://nas.local/Photos/2026");
    expect(displayPath("/Volumes/Photos", { ...photos, input: undefined })).toBeNull();
  });

  test("folders move to a new mount root", () => {
    expect(remapMountPath("/Volumes/Photos/2026", "/Volumes/Photos", "/Volumes/Photos-1")).toBe(
      "/Volumes/Photos-1/2026",
    );
    expect(remapMountPath("/Volumes/Photos", "/Volumes/Photos", "/Volumes/Photos-1")).toBe(
      "/Volumes/Photos-1",
    );
    expect(remapMountPath("Z:\\a\\b", "Z:\\", "Y:\\")).toBe("Y:\\a\\b");
    expect(remapMountPath("/elsewhere", "/Volumes/Photos", "/Volumes/Photos-1")).toBe(
      "/Volumes/Photos-1",
    );
  });
});
