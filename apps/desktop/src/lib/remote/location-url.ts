import type { Provider } from "./remote-locations.js";
import {
  emptyNetworkInput,
  needsUsername,
  allowsGuest,
  type FtpSecurity,
  type NetworkLocationInput,
  type NetworkProtocol,
  type WebdavSecurity,
} from "./network-locations.js";

export type ParsedLocationUrl =
  | { kind: "network"; input: NetworkLocationInput }
  | { kind: "cloud"; provider: Provider; bucket: string };

type SchemeDefaults = {
  protocol: NetworkProtocol;
  webdavSecurity?: WebdavSecurity;
  ftpSecurity?: FtpSecurity;
};

const SCHEMES: Record<string, SchemeDefaults> = {
  smb: { protocol: "smb" },
  cifs: { protocol: "smb" },
  nfs: { protocol: "nfs" },
  sftp: { protocol: "sftp" },
  ssh: { protocol: "sftp" },
  ftp: { protocol: "ftp", ftpSecurity: "plain" },
  ftps: { protocol: "ftp", ftpSecurity: "explicit" },
  http: { protocol: "webdav", webdavSecurity: "http" },
  dav: { protocol: "webdav", webdavSecurity: "http" },
  https: { protocol: "webdav", webdavSecurity: "https" },
  davs: { protocol: "webdav", webdavSecurity: "https" },
  webdav: { protocol: "webdav", webdavSecurity: "https" },
};

// scheme://[user[:password]@]host[:port][/path], host may be a bracketed IPv6 address.
const URL_PATTERN =
  /^([a-z][a-z0-9+.-]*):\/\/(?:([^:@/]*)(?::([^@/]*))?@)?(\[[^\]]+\]|[^/:[\]]+)(?::(\d+))?(\/.*)?$/i;
const UNC_PATTERN = /^\\\\([^\\/]+)(?:[\\/](.*))?$/;

function decode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

/** Fills the add-location form from a pasted address. Returns null for anything else, including bare host names. */
export function parseLocationUrl(text: string): ParsedLocationUrl | null {
  const value = text.trim();

  const unc = UNC_PATTERN.exec(value);
  if (unc) {
    const path = (unc[2] ?? "").replaceAll("\\", "/").replace(/\/+$/, "");
    return { kind: "network", input: { ...emptyNetworkInput("smb"), host: unc[1], path } };
  }

  const match = URL_PATTERN.exec(value);
  if (!match) return null;
  const [, rawScheme, user = "", password = "", rawHost, port = "", rawPath = ""] = match;
  const scheme = rawScheme.toLowerCase();
  if (scheme === "s3") return { kind: "cloud", provider: "aws", bucket: rawHost };

  const defaults = SCHEMES[scheme];
  if (!defaults) return null;
  const base = emptyNetworkInput(defaults.protocol);
  const path = decode(rawPath).replace(/\/+$/, "");
  const input: NetworkLocationInput = {
    ...base,
    webdavSecurity: defaults.webdavSecurity ?? base.webdavSecurity,
    ftpSecurity: defaults.ftpSecurity ?? base.ftpSecurity,
    host: rawHost.replace(/^\[|\]$/g, ""),
    port,
    path: defaults.protocol === "smb" ? path.replace(/^\/+/, "") : path,
    username: decode(user),
    password: decode(password),
  };
  if (input.username && allowsGuest(input.protocol)) input.auth = "password";
  return { kind: "network", input };
}

/** Address shown under the dialog title and copied from the sidebar. Never includes the password. */
export function formatLocationUrl(input: NetworkLocationInput): string {
  const scheme =
    input.protocol === "webdav"
      ? input.webdavSecurity
      : input.protocol === "ftp"
        ? input.ftpSecurity === "plain"
          ? "ftp"
          : "ftps"
        : input.protocol;
  const username = input.username.trim();
  const user = needsUsername(input) && username ? `${encodeURIComponent(username)}@` : "";
  const rawHost = input.host.trim() || "server";
  const host = rawHost.includes(":") ? `[${rawHost}]` : rawHost;
  const port = input.port.trim() ? `:${input.port.trim()}` : "";
  const path = input.path
    .trim()
    .replaceAll("\\", "/")
    .replace(/^\/+|\/+$/g, "");
  return `${scheme}://${user}${host}${port}${path ? `/${path}` : ""}`;
}
