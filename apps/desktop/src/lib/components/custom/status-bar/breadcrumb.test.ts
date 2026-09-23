import { describe, expect, test } from "bun:test";
import { breadcrumbSegments, collapseSegments } from "./breadcrumb";

describe("breadcrumb segments", () => {
  test("splits a local Unix path from the root", () => {
    expect(breadcrumbSegments("/home/sgc/Work", "/home/sgc/Work")).toEqual([
      { label: "/", path: "/" },
      { label: "home", path: "/home/" },
      { label: "sgc", path: "/home/sgc/" },
      { label: "Work", path: "/home/sgc/Work" },
    ]);
  });

  test("ignores a trailing separator", () => {
    expect(breadcrumbSegments("/home/", "/home/").map((s) => s.label)).toEqual(["/", "home"]);
    expect(breadcrumbSegments("/", "/")).toEqual([{ label: "/", path: "/" }]);
  });

  test("splits a Windows drive path", () => {
    expect(breadcrumbSegments("C:\\Users\\sgc", "C:\\Users\\sgc")).toEqual([
      { label: "C:", path: "C:\\" },
      { label: "Users", path: "C:\\Users\\" },
      { label: "sgc", path: "C:\\Users\\sgc" },
    ]);
  });

  test("keeps a UNC server and share as the root", () => {
    expect(breadcrumbSegments("\\\\nas\\media\\films", "\\\\nas\\media\\films")).toEqual([
      { label: "\\\\nas", path: "\\\\nas\\" },
      { label: "media", path: "\\\\nas\\media\\" },
      { label: "films", path: "\\\\nas\\media\\films" },
    ]);
  });

  test("maps S3 labels onto internal paths that carry the location id", () => {
    expect(breadcrumbSegments("s3://loc-1/bucket/photos/2024/", "s3://bucket/photos/2024/")).toEqual([
      { label: "s3://bucket", path: "s3://loc-1/bucket/" },
      { label: "photos", path: "s3://loc-1/bucket/photos/" },
      { label: "2024", path: "s3://loc-1/bucket/photos/2024/" },
    ]);
  });

  test("shows a location's bucket list as a single root", () => {
    expect(breadcrumbSegments("s3://loc-1/", "s3://")).toEqual([{ label: "s3://", path: "s3://loc-1/" }]);
  });

  test("labels a mounted share with its server address", () => {
    const mount = "/run/user/1000/gvfs/smb-share:server=nas,share=media";
    expect(breadcrumbSegments(`${mount}/films/new`, "smb://nas/media/films/new")).toEqual([
      { label: "smb://nas/media", path: `${mount}/` },
      { label: "films", path: `${mount}/films/` },
      { label: "new", path: `${mount}/films/new` },
    ]);
  });

  test("maps SFTP addresses onto the saved location", () => {
    expect(breadcrumbSegments("sftp://loc-2/var/www/", "sftp://me@host/var/www")).toEqual([
      { label: "sftp://me@host", path: "sftp://loc-2/" },
      { label: "var", path: "sftp://loc-2/var/" },
      { label: "www", path: "sftp://loc-2/var/www/" },
    ]);
  });

  test("has no segments without a path", () => {
    expect(breadcrumbSegments("", "")).toEqual([]);
  });
});

describe("collapsed segments", () => {
  const segments = ["/", "a", "b", "c", "d", "e"].map((label) => ({ label, path: label }));

  test("keeps short paths whole", () => {
    expect(collapseSegments(segments.slice(0, 4))).toEqual({ head: segments.slice(0, 4), hidden: [], tail: [] });
  });

  test("hides the middle of long paths behind the root and the last folders", () => {
    expect(collapseSegments(segments)).toEqual({
      head: [segments[0]],
      hidden: segments.slice(1, 3),
      tail: segments.slice(3),
    });
  });
});
