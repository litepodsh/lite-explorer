import { describe, expect, test } from "bun:test";
import { formatLocationUrl, parseLocationUrl } from "./location-url.js";
import { emptyNetworkInput, type NetworkLocationInput } from "./network-locations.js";

function network(text: string): NetworkLocationInput {
  const parsed = parseLocationUrl(text);
  if (parsed?.kind !== "network") throw new Error(`expected a network address: ${text}`);
  return parsed.input;
}

describe("parseLocationUrl", () => {
  test("SMB with user and share", () => {
    const input = network("smb://sebas@nas.local/Photos/2026/");
    expect([input.protocol, input.host, input.username, input.auth, input.path]).toEqual([
      "smb",
      "nas.local",
      "sebas",
      "password",
      "Photos/2026",
    ]);
    expect(network("cifs://nas/Media").protocol).toBe("smb");
  });

  test("Windows UNC paths", () => {
    const input = network("\\\\nas\\Photos\\2026");
    expect([input.protocol, input.host, input.path, input.auth]).toEqual([
      "smb",
      "nas",
      "Photos/2026",
      "guest",
    ]);
    expect(network("\\\\nas").path).toBe("");
  });

  test("SFTP with port and absolute path", () => {
    const input = network("sftp://pi@raspberrypi.local:2222/home/pi");
    expect([input.protocol, input.port, input.path, input.auth, input.username]).toEqual([
      "sftp",
      "2222",
      "/home/pi",
      "password",
      "pi",
    ]);
    expect(network("ssh://server").protocol).toBe("sftp");
  });

  test("FTP security comes from the scheme", () => {
    expect(network("ftp://files.lan").ftpSecurity).toBe("plain");
    expect(network("ftps://files.lan").ftpSecurity).toBe("explicit");
  });

  test("HTTP schemes are WebDAV", () => {
    const secure = network("https://cloud.example.com/remote.php/dav");
    expect([secure.protocol, secure.webdavSecurity, secure.path]).toEqual([
      "webdav",
      "https",
      "/remote.php/dav",
    ]);
    expect(network("http://nas:5005").webdavSecurity).toBe("http");
    expect(network("davs://nas").webdavSecurity).toBe("https");
    expect(network("dav://nas").webdavSecurity).toBe("http");
    expect(network("webdav://nas").webdavSecurity).toBe("https");
  });

  test("NFS export path", () => {
    const input = network("nfs://storage/srv/media");
    expect([input.protocol, input.path, input.auth]).toEqual(["nfs", "/srv/media", "guest"]);
  });

  test("encoded credentials are decoded", () => {
    const input = network("ftp://bob:p%40ss@host/");
    expect([input.username, input.password]).toEqual(["bob", "p@ss"]);
    expect(network("smb://DOMAIN%5Csebas@nas/Share").username).toBe("DOMAIN\\sebas");
  });

  test("IPv6 hosts lose their brackets", () => {
    expect(network("smb://[fe80::1]/Share").host).toBe("fe80::1");
  });

  test("S3 buckets", () => {
    expect(parseLocationUrl("s3://assets")).toEqual({
      kind: "cloud",
      provider: "aws",
      bucket: "assets",
    });
  });

  test("anything else is not an address", () => {
    expect(parseLocationUrl("afp://nas/share")).toBeNull();
    expect(parseLocationUrl("nas.local")).toBeNull();
    expect(parseLocationUrl("")).toBeNull();
    expect(parseLocationUrl("smb://")).toBeNull();
  });
});

describe("formatLocationUrl", () => {
  test("never includes the password", () => {
    const input = {
      ...emptyNetworkInput("sftp"),
      host: "pi",
      username: "pi",
      password: "secret",
      path: "/home/pi",
    };
    expect(formatLocationUrl(input)).toBe("sftp://pi@pi/home/pi");
  });

  test("guests have no user part, and an empty host shows a placeholder", () => {
    expect(formatLocationUrl({ ...emptyNetworkInput("smb"), username: "ignored" })).toBe(
      "smb://server",
    );
  });

  test("scheme follows security", () => {
    expect(
      formatLocationUrl({ ...emptyNetworkInput("webdav"), host: "dav", webdavSecurity: "http" }),
    ).toBe("http://dav");
    expect(
      formatLocationUrl({ ...emptyNetworkInput("ftp"), host: "f", ftpSecurity: "implicit" }),
    ).toBe("ftps://f");
    expect(
      formatLocationUrl({
        ...emptyNetworkInput("ftp"),
        host: "f",
        ftpSecurity: "plain",
        port: "2121",
      }),
    ).toBe("ftp://f:2121");
  });

  test("round trip", () => {
    for (const text of [
      "smb://DOMAIN%5Csebas@nas/Photos/2026",
      "smb://[fe80::1]/Share",
      "nfs://storage/srv/media",
    ]) {
      expect(formatLocationUrl(network(text))).toBe(text);
    }
  });
});
