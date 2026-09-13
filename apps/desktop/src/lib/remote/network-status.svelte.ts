import type { Location } from "$lib/tabs/tabs.js";
import {
  connectNetworkLocation,
  describeError,
  disconnectNetworkLocation,
  getNetworkLocation,
  isInsideMount,
  isNetworkPath,
  mountNetworkShare,
  needsTrust,
  networkConnections,
  toConnectError,
  type ConnectError,
  type NetworkLocationInput,
} from "./network-locations.js";
import { displayPath, findOwner, type KnownLocation, type MountedShare } from "./network-paths.js";

/** Connection state of a saved network location, shown on its sidebar row. */
export type LocationStatus =
  | { state: "idle" }
  | { state: "connecting" }
  | { state: "connected"; mountPath: string }
  | { state: "error"; message: string }
  | { state: "locked" };

export type ConnectOutcome =
  | { kind: "connected"; mountPath: string }
  /** The server needs a password that isn't saved, or rejected the one given. */
  | { kind: "password"; error: ConnectError }
  /** The server presented a key or certificate that isn't trusted yet. */
  | { kind: "trust"; error: ConnectError & { trust: NonNullable<ConnectError["trust"]> } }
  | { kind: "failed"; error: ConnectError };

const IDLE: LocationStatus = { state: "idle" };

/** Connection states by location path. Not persisted; `refresh` picks up existing mounts. */
class NetworkStatus {
  statuses = $state<Record<string, LocationStatus>>({});
  /** Saved network locations from the sidebar, set by the page. */
  locations = $state<Location[]>([]);
  /** Last mount folder per location path, kept after a disconnect so open tabs still match. */
  #mounts = $state<Record<string, string>>({});
  /** Mounted shares of SMB locations without a share, by location path. */
  #shares = $state<Record<string, MountedShare[]>>({});
  /** Settings per location path, loaded once for addresses. */
  #inputs = $state<Record<string, NetworkLocationInput>>({});
  #loading = new Set<string>();

  #known(): KnownLocation[] {
    return this.locations
      .filter((location) => isNetworkPath(location.path))
      .map((location) => ({
        location,
        mountPath: this.#mounts[location.path],
        shares: this.#shares[location.path],
        input: this.#inputs[location.path],
      }));
  }

  /** The saved location an SFTP/FTP path or a folder inside a mount belongs to. */
  ownerOf(path: string): Location | null {
    return path ? (findOwner(path, this.#known())?.location ?? null) : null;
  }

  /** Last mount folder of a location, even when it's disconnected now. */
  lastMount(location: Location): string | null {
    return this.#mounts[location.path] ?? null;
  }

  /** Address like `sftp://pi@raspberry.local/home/pi` for a path inside a network location. */
  address(path: string): string | null {
    const owner = path ? findOwner(path, this.#known()) : null;
    if (!owner) return null;
    if (!owner.input) {
      this.#loadInput(owner.location.path);
      return null;
    }
    return displayPath(path, owner);
  }

  #loadInput(path: string) {
    if (this.#loading.has(path)) return;
    this.#loading.add(path);
    getNetworkLocation(path)
      .then((input) => (this.#inputs[path] = input))
      .catch(() => {})
      .finally(() => this.#loading.delete(path));
  }

  /**
   * Asks the system whether a location is still connected, after a listing inside it failed.
   * A share ejected elsewhere or a dropped server session goes back to idle.
   */
  async isConnected(location: Location): Promise<boolean> {
    const connections = await networkConnections();
    const connection = connections.find((candidate) => candidate.location === location.path);
    if (connection) {
      this.#connected(location.path, connection.path);
      return true;
    }
    if (this.status(location.path).state === "connected") this.statuses[location.path] = IDLE;
    return false;
  }

  #connected(path: string, mountPath: string) {
    this.statuses[path] = { state: "connected", mountPath };
    this.#mounts[path] = mountPath;
  }

  status(path: string): LocationStatus {
    return this.statuses[path] ?? IDLE;
  }

  /** Mount folder of a connected location that contains `path`, if any. */
  mountFor(path: string): string | null {
    for (const status of Object.values(this.statuses)) {
      if (status.state === "connected" && isInsideMount(path, status.mountPath))
        return status.mountPath;
    }
    for (const shares of Object.values(this.#shares)) {
      const share = shares.find((candidate) => isInsideMount(path, candidate.path));
      if (share) return share.path;
    }
    return null;
  }

  /** The remembered share of an SMB server location that contains `path`. */
  shareAt(location: Location, path: string): MountedShare | null {
    return (
      (this.#shares[location.path] ?? []).find((share) => isInsideMount(path, share.path)) ?? null
    );
  }

  /** The SMB server location whose mounted share has its root at `path`. */
  serverOfShare(path: string): Location | null {
    const trimmed = path.replace(/[\\/]+$/, "");
    for (const [locationPath, shares] of Object.entries(this.#shares)) {
      if (shares.some((share) => share.path.replace(/[\\/]+$/, "") === trimmed)) {
        return this.locations.find((location) => location.path === locationPath) ?? null;
      }
    }
    return null;
  }

  /** Records the mounted shares listed for an SMB location without a share. */
  rememberShares(location: Location, shares: MountedShare[]) {
    const known = this.#shares[location.path] ?? [];
    const merged = [
      ...shares,
      ...known.filter((old) => !shares.some((share) => share.name === old.name)),
    ];
    this.#shares[location.path] = merged;
  }

  /** Mounts one share listed by an SMB location without a share. */
  async mountShare(
    location: Location,
    sharePath: string,
    options: { password?: string; remember?: boolean } = {},
  ): Promise<ConnectOutcome> {
    try {
      const connection = await mountNetworkShare(sharePath, options);
      const name = sharePath.split("/").filter(Boolean).pop() ?? "";
      this.rememberShares(location, [{ name, path: connection.path }]);
      if (this.status(location.path).state !== "connected")
        this.#connected(location.path, location.path);
      return { kind: "connected", mountPath: connection.path };
    } catch (reason) {
      const error = toConnectError(reason);
      if (error.kind === "auth") return { kind: "password", error };
      return { kind: "failed", error };
    }
  }

  async connect(
    location: Location,
    options: { password?: string; remember?: boolean } = {},
  ): Promise<ConnectOutcome> {
    this.statuses[location.path] = { state: "connecting" };
    try {
      const connection = await connectNetworkLocation(location.path, options);
      this.#connected(location.path, connection.path);
      return { kind: "connected", mountPath: connection.path };
    } catch (reason) {
      const error = toConnectError(reason);
      if (error.kind === "auth") {
        this.statuses[location.path] = { state: "locked" };
        return { kind: "password", error };
      }
      if (needsTrust(error)) {
        this.statuses[location.path] = { state: "idle" };
        return { kind: "trust", error };
      }
      this.statuses[location.path] = { state: "error", message: describeError(error) };
      return { kind: "failed", error };
    }
  }

  async disconnect(location: Location) {
    await disconnectNetworkLocation(location.path);
    this.statuses[location.path] = IDLE;
  }

  /** Marks locations the system already has mounted, for example after a restart. */
  async refresh() {
    for (const connection of await networkConnections()) {
      this.#connected(connection.location, connection.path);
    }
  }

  /** Drops cached settings after an edit, so addresses show the new server. */
  invalidate(path: string) {
    delete this.#inputs[path];
  }

  /** Clears the state of a removed or edited location. */
  forget(path: string) {
    delete this.statuses[path];
    delete this.#mounts[path];
    delete this.#shares[path];
    delete this.#inputs[path];
  }
}

export const networkStatus = new NetworkStatus();
