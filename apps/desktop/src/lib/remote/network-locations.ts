import { invoke } from "@tauri-apps/api/core";
import type { Location } from "$lib/tabs/tabs.js";

export type NetworkProtocol = "smb" | "nfs" | "webdav" | "sftp" | "ftp";
export type NetworkAuth = "guest" | "password";
export type WebdavSecurity = "http" | "https";
export type FtpSecurity = "explicit" | "implicit" | "plain";

/** Form state for a network location. `port` stays a string while the user types. */
export type NetworkLocationInput = {
  protocol: NetworkProtocol;
  name: string;
  host: string;
  port: string;
  path: string;
  auth: NetworkAuth;
  username: string;
  password: string;
  rememberPassword: boolean;
  webdavSecurity: WebdavSecurity;
  ftpSecurity: FtpSecurity;
};

/** Mirrors `NetworkLocationInput` in `src-tauri/src/network.rs`. */
export type NetworkLocationPayload = {
  protocol: NetworkProtocol;
  name: string;
  host: string;
  port: number | null;
  path: string;
  auth: NetworkAuth;
  username: string;
  password: string;
  security: WebdavSecurity | FtpSecurity | null;
  rememberPassword: boolean;
};

export type NetworkField = "host" | "port" | "path" | "username";

export type ConnectErrorKind =
  | "auth"
  | "unreachable"
  | "timeout"
  | "not_found"
  | "host_key_unknown"
  | "host_key_changed"
  | "certificate_untrusted"
  | "unsupported"
  | "invalid"
  | "other";

/** A server key or certificate the user can trust. `changed` means a different one was trusted before. */
export type TrustRequest = {
  protocol: NetworkProtocol;
  host: string;
  port: number;
  algorithm: string;
  fingerprint: string;
  changed: boolean;
};

/** Error shape of the network commands. `trust` comes with host key and certificate kinds. */
export type ConnectError = { kind: ConnectErrorKind; message: string; trust?: TrustRequest };

export type ConnectionCheck = { message: string };

/** SMB server found on the local network by Bonjour or by probing port 445. */
export type DiscoveredServer = {
  scan: number;
  name: string;
  /** Bonjour host name (`nas.local`), or the IP address for probed hosts. */
  host: string;
  addresses: string[];
  port: number;
  source: "bonjour" | "scan";
};

/**
 * Adds a scan result, one entry per machine. Bonjour entries win over probed IP addresses
 * because they carry a name; a probed address already covered by Bonjour is dropped.
 */
export function mergeDiscovered(
  servers: DiscoveredServer[],
  server: DiscoveredServer,
): DiscoveredServer[] {
  const shares = (a: DiscoveredServer, b: DiscoveredServer) =>
    a.host === b.host || a.addresses.some((address) => b.addresses.includes(address));
  if (server.source === "scan") {
    return servers.some((known) => shares(known, server)) ? servers : [...servers, server];
  }
  const others = servers.filter(
    (known) =>
      !(known.source === "scan" && shares(known, server)) &&
      !(known.source === "bonjour" && known.host === server.host),
  );
  const merged = [...others, server];
  return merged.sort((a, b) => Number(a.source === "scan") - Number(b.source === "scan"));
}

/** A saved location (`location` is its `smb://…` path) and the local folder it's mounted at. */
export type NetworkConnection = { location: string; path: string };

export const NETWORK_PROTOCOLS: NetworkProtocol[] = ["smb", "nfs", "webdav", "sftp", "ftp"];

const ERROR_KINDS: ConnectErrorKind[] = [
  "auth",
  "unreachable",
  "timeout",
  "not_found",
  "host_key_unknown",
  "host_key_changed",
  "certificate_untrusted",
  "unsupported",
  "invalid",
  "other",
];

export function isNetworkProtocol(kind: string): kind is NetworkProtocol {
  return (NETWORK_PROTOCOLS as string[]).includes(kind);
}

const PATH_PATTERN = /^(smb|nfs|webdav|sftp|ftp):\/\/[^/]+/;
const SERVER_PATTERN = /^(sftp|ftp):\/\/[^/]+/;

/** SFTP and FTP paths, browsed through the app instead of a system mount. */
export function isServerPath(path: string): boolean {
  return SERVER_PATTERN.test(path);
}

/** Saved network locations have paths like `smb://<location-id>/<share>`; see `src-tauri/src/network.rs`. */
export function isNetworkPath(path: string): boolean {
  return PATH_PATTERN.test(path);
}

export function emptyNetworkInput(protocol: NetworkProtocol = "smb"): NetworkLocationInput {
  return {
    protocol,
    name: "",
    host: "",
    port: "",
    path: "",
    auth: protocol === "sftp" ? "password" : "guest",
    username: "",
    password: "",
    rememberPassword: true,
    webdavSecurity: "https",
    ftpSecurity: "explicit",
  };
}

/** Switch protocol, keeping server, name and credentials. Port and path reset because their meaning changes. */
export function withProtocol(
  input: NetworkLocationInput,
  protocol: NetworkProtocol,
): NetworkLocationInput {
  const same = protocol === input.protocol;
  return {
    ...input,
    protocol,
    port: same ? input.port : "",
    path: same ? input.path : "",
    auth: protocol === "sftp" ? "password" : protocol === "nfs" ? "guest" : input.auth,
  };
}

/** SMB, NFS and WebDAV are mounted by the operating system and browsed as local folders. */
export function isMountable(kind: string): boolean {
  return kind === "smb" || kind === "nfs" || kind === "webdav";
}

/** Whether `path` is `root` or inside it. Works with `/` and Windows backslash separators. */
export function isInsideMount(path: string, root: string): boolean {
  if (!path || !root) return false;
  if (path === root) return true;
  const separator = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  const prefix = root.endsWith(separator) ? root : root + separator;
  return path.startsWith(prefix);
}

export function hasAuth(protocol: NetworkProtocol): boolean {
  return protocol !== "nfs";
}

export function allowsGuest(protocol: NetworkProtocol): boolean {
  return protocol === "smb" || protocol === "webdav" || protocol === "ftp";
}

export function needsUsername(input: NetworkLocationInput): boolean {
  return input.protocol === "sftp" || (allowsGuest(input.protocol) && input.auth === "password");
}

export function defaultPort(input: NetworkLocationInput): number {
  switch (input.protocol) {
    case "smb":
      return 445;
    case "nfs":
      return 2049;
    case "webdav":
      return input.webdavSecurity === "http" ? 80 : 443;
    case "sftp":
      return 22;
    case "ftp":
      return input.ftpSecurity === "implicit" ? 990 : 21;
  }
}

export function pathLabel(protocol: NetworkProtocol): string {
  if (protocol === "smb") return "Share";
  if (protocol === "nfs") return "Export path";
  return "Folder";
}

export function pathPlaceholder(protocol: NetworkProtocol): string {
  if (protocol === "smb") return "Photos";
  if (protocol === "nfs") return "/srv/exports/media";
  if (protocol === "webdav") return "/remote.php/dav/files/me";
  return "/home/me";
}

/** Name used when the Name field is empty: the last path segment, else the server. Matches the backend. */
export function fallbackName(input: NetworkLocationInput): string {
  return input.path.split(/[\\/]/).filter(Boolean).pop() || input.host.trim() || "Server";
}

/** Fields that are required and empty, or malformed. The backend validates everything again. */
export function missingFields(input: NetworkLocationInput): NetworkField[] {
  const missing: NetworkField[] = [];
  if (!input.host.trim()) missing.push("host");
  const port = input.port.trim();
  if (port && !(/^\d{1,5}$/.test(port) && Number(port) >= 1 && Number(port) <= 65535))
    missing.push("port");
  if (input.protocol === "nfs" && !input.path.trim()) missing.push("path");
  if (needsUsername(input) && !input.username.trim()) missing.push("username");
  return missing;
}

export function toPayload(input: NetworkLocationInput): NetworkLocationPayload {
  const port = input.port.trim();
  return {
    protocol: input.protocol,
    name: input.name,
    host: input.host,
    port: port ? Number(port) : null,
    path: input.path,
    auth: input.protocol === "sftp" ? "password" : input.auth,
    username: input.username,
    password: input.password,
    security:
      input.protocol === "webdav"
        ? input.webdavSecurity
        : input.protocol === "ftp"
          ? input.ftpSecurity
          : null,
    rememberPassword: input.rememberPassword,
  };
}

export function fromPayload(payload: NetworkLocationPayload): NetworkLocationInput {
  const base = emptyNetworkInput(payload.protocol);
  const security = payload.security;
  return {
    ...base,
    name: payload.name,
    host: payload.host,
    port: payload.port === null ? "" : String(payload.port),
    path: payload.path,
    auth: payload.auth,
    username: payload.username,
    rememberPassword: payload.rememberPassword,
    webdavSecurity: security === "http" || security === "https" ? security : base.webdavSecurity,
    ftpSecurity:
      security === "explicit" || security === "implicit" || security === "plain"
        ? security
        : base.ftpSecurity,
  };
}

export function toConnectError(error: unknown): ConnectError {
  if (typeof error === "object" && error !== null && "kind" in error && "message" in error) {
    const candidate = error as ConnectError;
    if (ERROR_KINDS.includes(candidate.kind)) return candidate;
  }
  const message = error instanceof Error ? error.message : typeof error === "string" ? error : "";
  return { kind: "other", message };
}

const ERROR_COPY: Partial<Record<ConnectErrorKind, string>> = {
  auth: "The server rejected the username or password.",
  host_key_unknown:
    "First connection to this server. Check that its fingerprint matches before trusting it.",
  host_key_changed:
    "This server’s host key changed since the last connection. Someone could be intercepting it, or the server was reinstalled.",
  certificate_untrusted: "This computer doesn’t trust the server’s certificate.",
};

/** User-facing copy. Kinds whose backend message already names the server keep that message. */
export function describeError(error: ConnectError): string {
  return error.message || ERROR_COPY[error.kind] || "Couldn’t connect to the server.";
}

export function needsTrust(error: ConnectError): error is ConnectError & { trust: TrustRequest } {
  return (
    Boolean(error.trust) &&
    (error.kind === "host_key_unknown" ||
      error.kind === "host_key_changed" ||
      error.kind === "certificate_untrusted")
  );
}

export function trustNetworkHost(trust: TrustRequest): Promise<void> {
  return call<void>("trust_network_host", { trust });
}

async function call<T>(command: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toConnectError(error);
  }
}

export function testNetworkLocation(input: NetworkLocationInput): Promise<ConnectionCheck> {
  return call<ConnectionCheck>("test_network_location", { input: toPayload(input) });
}

export function addNetworkLocation(input: NetworkLocationInput): Promise<Location> {
  return call<Location>("add_network_location", { input: toPayload(input) });
}

export function updateNetworkLocation(
  path: string,
  input: NetworkLocationInput,
): Promise<Location> {
  return call<Location>("update_network_location", { path, input: toPayload(input) });
}

export function removeNetworkLocation(location: Location): Promise<void> {
  return call<void>("remove_network_location", { path: location.path });
}

/** Mounts a saved location (or reuses its mount). Without a password the saved one is used. */
export function connectNetworkLocation(
  path: string,
  options: { username?: string; password?: string; remember?: boolean } = {},
): Promise<NetworkConnection> {
  return call<NetworkConnection>("connect_network_location", {
    path,
    username: options.username ?? null,
    password: options.password ?? null,
    remember: options.remember ?? null,
  });
}

/** Mounts one share (`smb://<id>/<share>`) of an SMB location without a share. */
export function mountNetworkShare(
  path: string,
  options: { username?: string; password?: string; remember?: boolean } = {},
): Promise<NetworkConnection> {
  return call<NetworkConnection>("mount_network_share", {
    path,
    username: options.username ?? null,
    password: options.password ?? null,
    remember: options.remember ?? null,
  });
}

export function disconnectNetworkLocation(path: string): Promise<void> {
  return call<void>("disconnect_network_location", { path });
}

/** Starts a scan; results arrive as `network-server` events tagged with `scan`. */
export function scanSmbServers(scan: number): Promise<void> {
  return call<void>("scan_smb_servers", { scan });
}

export function stopNetworkScan(): Promise<void> {
  return call<void>("stop_network_scan", {});
}

export function openLocalNetworkSettings(): Promise<void> {
  return call<void>("open_local_network_settings", {});
}

/** Saved locations that are mounted right now. */
export function networkConnections(): Promise<NetworkConnection[]> {
  return call<NetworkConnection[]>("network_connections", {});
}

export async function getNetworkLocation(path: string): Promise<NetworkLocationInput> {
  return fromPayload(await call<NetworkLocationPayload>("network_location", { path }));
}
