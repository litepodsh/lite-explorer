import type { Location } from "$lib/tabs/tabs.js";
import { formatLocationUrl } from "./location-url.js";
import {
  isInsideMount,
  isNetworkPath,
  isServerPath,
  type NetworkLocationInput,
} from "./network-locations.js";

/** A mounted share of an SMB location without a share, and its local folder. */
export type MountedShare = { name: string; path: string };

/** What the app knows about a saved network location while it runs. */
export type KnownLocation = {
  location: Location;
  /** Local folder of its last mount. Kept after a disconnect so open tabs can be matched. */
  mountPath?: string;
  /** Shares opened from an SMB location without a share. */
  shares?: MountedShare[];
  /** Settings shown in addresses: `smb://nas.local/Photos`. */
  input?: NetworkLocationInput;
};

const ROOT_PATTERN = /^(smb|nfs|webdav|sftp|ftp):\/\/[^/]+/;

/** `sftp://<id>` part of a network path. */
function locationRoot(path: string): string | null {
  return ROOT_PATTERN.exec(path)?.[0] ?? null;
}

/**
 * The saved location that `path` belongs to. SFTP and FTP paths match by location id, and
 * local folders match the deepest mount that contains them.
 */
export function findOwner(path: string, known: KnownLocation[]): KnownLocation | null {
  if (isNetworkPath(path)) {
    const root = locationRoot(path);
    return known.find((candidate) => locationRoot(candidate.location.path) === root) ?? null;
  }
  let owner: KnownLocation | null = null;
  let depth = 0;
  for (const candidate of known) {
    for (const root of mountRoots(candidate)) {
      if (root.path.length > depth && isInsideMount(path, root.path)) {
        owner = candidate;
        depth = root.path.length;
      }
    }
  }
  return owner;
}

/** Local folders of a location with the address path each one stands for. */
function mountRoots(known: KnownLocation): MountedShare[] {
  const roots = known.shares ?? [];
  // `smb://<id>/` paths aren't local folders; a location without a share lists its shares instead.
  return known.mountPath && !isNetworkPath(known.mountPath)
    ? [{ name: "", path: known.mountPath }, ...roots]
    : roots;
}

/** Path below `root`, as `/`-separated segments without leading or trailing separators. */
function relativeSegments(path: string, root: string): string[] {
  return path.slice(root.length).split(/[\\/]/).filter(Boolean);
}

/**
 * Address shown instead of an internal path: `sftp://pi@raspberry.local/home/pi/notes` for
 * `sftp://<id>/home/pi/notes`, or `smb://nas.local/Photos/2026` for a folder inside the mount.
 */
export function displayPath(path: string, owner: KnownLocation): string | null {
  const input = owner.input;
  if (!input) return null;
  if (isServerPath(path)) {
    const root = locationRoot(path) ?? "";
    const remote = relativeSegments(path, root).join("/");
    return `${formatLocationUrl({ ...input, path: "" })}/${remote}`;
  }
  if (isNetworkPath(path)) {
    const root = locationRoot(path) ?? "";
    const below = relativeSegments(path, root);
    const server = formatLocationUrl({ ...input, path: "" });
    return below.length ? `${server}/${below.join("/")}` : formatLocationUrl(input);
  }
  let match: MountedShare | null = null;
  for (const root of mountRoots(owner)) {
    if (isInsideMount(path, root.path) && root.path.length > (match?.path.length ?? -1))
      match = root;
  }
  if (!match) return null;
  const below = relativeSegments(path, match.path);
  const address = match.name
    ? `${formatLocationUrl({ ...input, path: "" })}/${match.name}`
    : formatLocationUrl(input);
  return below.length ? `${address}/${below.join("/")}` : address;
}

/** The same folder under a new mount root, for example `/Volumes/Photos-1` after remounting. */
export function remapMountPath(path: string, from: string, to: string): string {
  if (!isInsideMount(path, from)) return to;
  const below = relativeSegments(path, from);
  if (!below.length) return to;
  const separator = to.includes("\\") && !to.includes("/") ? "\\" : "/";
  const base = to.endsWith(separator) ? to.slice(0, -1) : to;
  return [base, ...below].join(separator);
}
