import { describe, expect, test } from "bun:test";
import {
  defaultPort,
  describeError,
  emptyNetworkInput,
  fallbackName,
  fromPayload,
  isInsideMount,
  mergeDiscovered,
  isMountable,
  isNetworkPath,
  isServerPath,
  needsTrust,
  missingFields,
  toConnectError,
  toPayload,
  withProtocol,
} from "./network-locations.js";

describe("network locations", () => {
  test("SFTP starts with password auth, the rest as guest", () => {
    expect(emptyNetworkInput("sftp").auth).toBe("password");
    expect(emptyNetworkInput("smb").auth).toBe("guest");
    expect(emptyNetworkInput().protocol).toBe("smb");
  });

  test("default ports follow protocol and security", () => {
    expect(defaultPort(emptyNetworkInput("smb"))).toBe(445);
    expect(defaultPort(emptyNetworkInput("nfs"))).toBe(2049);
    expect(defaultPort(emptyNetworkInput("webdav"))).toBe(443);
    expect(defaultPort({ ...emptyNetworkInput("webdav"), webdavSecurity: "http" })).toBe(80);
    expect(defaultPort(emptyNetworkInput("sftp"))).toBe(22);
    expect(defaultPort(emptyNetworkInput("ftp"))).toBe(21);
    expect(defaultPort({ ...emptyNetworkInput("ftp"), ftpSecurity: "implicit" })).toBe(990);
  });

  test("required and malformed fields depend on protocol", () => {
    expect(missingFields(emptyNetworkInput("smb"))).toEqual(["host"]);
    expect(missingFields(emptyNetworkInput("nfs"))).toEqual(["host", "path"]);
    expect(missingFields(emptyNetworkInput("sftp"))).toEqual(["host", "username"]);
    const smb = { ...emptyNetworkInput("smb"), host: "nas" };
    expect(missingFields({ ...smb, auth: "password" })).toEqual(["username"]);
    for (const port of ["abc", "0", "65536", "12 3"]) {
      expect(missingFields({ ...smb, port })).toEqual(["port"]);
    }
    expect(missingFields({ ...smb, port: " 2222 " })).toEqual([]);
  });

  test("switching protocol keeps server and credentials", () => {
    const draft = {
      ...emptyNetworkInput("smb"),
      name: "NAS",
      host: "nas.local",
      port: "1445",
      path: "Photos",
      auth: "password" as const,
      username: "sebas",
      password: "pw",
    };
    const sftp = withProtocol(draft, "sftp");
    expect([sftp.protocol, sftp.name, sftp.host, sftp.username, sftp.password]).toEqual([
      "sftp",
      "NAS",
      "nas.local",
      "sebas",
      "pw",
    ]);
    expect([sftp.port, sftp.path, sftp.auth]).toEqual(["", "", "password"]);
    expect(withProtocol(draft, "nfs").auth).toBe("guest");
    expect(withProtocol(draft, "smb").path).toBe("Photos");
    expect(withProtocol({ ...draft, auth: "guest" }, "webdav").auth).toBe("guest");
  });

  test("fallback name is the last path segment, then the server", () => {
    expect(fallbackName({ ...emptyNetworkInput("smb"), host: "nas", path: "Photos/2026/" })).toBe(
      "2026",
    );
    expect(fallbackName({ ...emptyNetworkInput("smb"), host: " nas " })).toBe("nas");
    expect(fallbackName(emptyNetworkInput("smb"))).toBe("Server");
  });

  test("payload converts port and picks security for the protocol", () => {
    const webdav = {
      ...emptyNetworkInput("webdav"),
      host: "dav",
      port: "8443",
      ftpSecurity: "plain" as const,
    };
    const payload = toPayload(webdav);
    expect([payload.port, payload.security]).toEqual([8443, "https"]);
    expect(toPayload(emptyNetworkInput("smb")).security).toBeNull();
    expect(toPayload(emptyNetworkInput("smb")).port).toBeNull();
    expect(toPayload({ ...emptyNetworkInput("sftp"), auth: "guest" }).auth).toBe("password");
  });

  test("saved settings load into the form without the password", () => {
    const input = fromPayload({
      protocol: "ftp",
      name: "Backups",
      host: "ftp.lan",
      port: 990,
      path: "/backups",
      auth: "password",
      username: "backup",
      password: "leaked?",
      security: "implicit",
      rememberPassword: true,
    });
    expect([input.port, input.ftpSecurity, input.webdavSecurity, input.password]).toEqual([
      "990",
      "implicit",
      "https",
      "",
    ]);
    expect(toPayload(input).security).toBe("implicit");
  });

  test("errors from the backend keep their kind; anything else becomes other", () => {
    expect(toConnectError({ kind: "timeout", message: "No answer" })).toEqual({
      kind: "timeout",
      message: "No answer",
    });
    expect(toConnectError("Keychain: denied")).toEqual({
      kind: "other",
      message: "Keychain: denied",
    });
    expect(toConnectError(new Error("boom"))).toEqual({ kind: "other", message: "boom" });
    expect(toConnectError({ kind: "nope", message: "x" })).toEqual({ kind: "other", message: "" });
  });

  test("error copy", () => {
    expect(describeError({ kind: "auth", message: "" })).toBe(
      "The server rejected the username or password.",
    );
    expect(
      describeError({ kind: "unreachable", message: "nas refused connections on port 445." }),
    ).toBe("nas refused connections on port 445.");
    expect(describeError({ kind: "other", message: "" })).toBe("Couldn’t connect to the server.");
  });

  test("network paths", () => {
    expect(isNetworkPath("smb://abc/Photos")).toBe(true);
    expect(isNetworkPath("ftp://abc")).toBe(true);
    expect(isNetworkPath("s3://abc/")).toBe(false);
    expect(isNetworkPath("smb:///Photos")).toBe(false);
    expect(isNetworkPath("/Users/me")).toBe(false);
  });

  test("mountable kinds", () => {
    expect(["smb", "nfs", "webdav"].map(isMountable)).toEqual([true, true, true]);
    expect(["sftp", "ftp", "s3", "home"].map(isMountable)).toEqual([false, false, false, false]);
  });

  test("paths inside a mount", () => {
    expect(isInsideMount("/Volumes/Photos", "/Volumes/Photos")).toBe(true);
    expect(isInsideMount("/Volumes/Photos/2026", "/Volumes/Photos")).toBe(true);
    expect(isInsideMount("/Volumes/Photos Old", "/Volumes/Photos")).toBe(false);
    expect(isInsideMount("/Volumes/Photos/2026", "/Volumes/Photos/")).toBe(true);
    expect(isInsideMount("\\\\nas\\Photos\\2026", "\\\\nas\\Photos")).toBe(true);
    expect(isInsideMount("\\\\nas\\PhotosOld", "\\\\nas\\Photos")).toBe(false);
    expect(isInsideMount("Z:\\movies", "Z:\\")).toBe(true);
    expect(isInsideMount("", "/Volumes/Photos")).toBe(false);
  });

  test("discovered servers merge by machine", () => {
    const probed = {
      scan: 1,
      name: "192.168.1.65",
      host: "192.168.1.65",
      addresses: ["192.168.1.65"],
      port: 445,
      source: "scan" as const,
    };
    const nas = {
      scan: 1,
      name: "Garzon NAS",
      host: "garzon-nas.local",
      addresses: ["192.168.1.65"],
      port: 445,
      source: "bonjour" as const,
    };
    const pc = {
      ...probed,
      name: "192.168.1.70",
      host: "192.168.1.70",
      addresses: ["192.168.1.70"],
    };

    let servers = mergeDiscovered([], probed);
    servers = mergeDiscovered(servers, pc);
    servers = mergeDiscovered(servers, nas);
    expect(servers.map((server) => server.name)).toEqual(["Garzon NAS", "192.168.1.70"]);
    expect(mergeDiscovered(servers, probed).length).toBe(2);
    expect(mergeDiscovered(servers, { ...nas, addresses: [] }).length).toBe(2);
  });

  test("server paths and trust errors", () => {
    expect(isServerPath("sftp://abc/home/pi")).toBe(true);
    expect(isServerPath("ftp://abc")).toBe(true);
    expect(isServerPath("smb://abc/Photos")).toBe(false);
    const trust = {
      protocol: "sftp" as const,
      host: "pi",
      port: 22,
      algorithm: "ssh-ed25519",
      fingerprint: "SHA256:x",
      changed: false,
    };
    expect(needsTrust({ kind: "host_key_unknown", message: "", trust })).toBe(true);
    expect(needsTrust({ kind: "host_key_unknown", message: "" })).toBe(false);
    expect(needsTrust({ kind: "auth", message: "", trust })).toBe(false);
    expect(
      describeError({ kind: "host_key_changed", message: "The host key of pi changed." }),
    ).toBe("The host key of pi changed.");
  });
});
