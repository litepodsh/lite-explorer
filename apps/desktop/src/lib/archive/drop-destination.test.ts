import { describe, expect, test } from "bun:test";
import { resolveDestination, type PaneFolder } from "./drop-destination.js";

const panes = new Map<string, PaneFolder>([
  [
    "left",
    {
      folder: () => "/Users/me/Downloads",
      isFolder: (path) => path === "/Users/me/Downloads/photos",
    },
  ],
  ["remote", { folder: () => null, isFolder: () => false }],
]);

describe("resolveDestination", () => {
  test("extracts into a favorite", () => {
    expect(resolveDestination({ kind: "favorite", path: "/Users/me/Projects" }, panes)).toBe(
      "/Users/me/Projects",
    );
  });

  test("extracts into a local pane's current folder", () => {
    expect(resolveDestination({ kind: "pane", paneId: "left" }, panes)).toBe("/Users/me/Downloads");
  });

  test("extracts into a folder row", () => {
    expect(
      resolveDestination(
        { kind: "entry", paneId: "left", path: "/Users/me/Downloads/photos" },
        panes,
      ),
    ).toBe("/Users/me/Downloads/photos");
  });

  test("uses the pane's folder for a file row", () => {
    expect(
      resolveDestination(
        { kind: "entry", paneId: "left", path: "/Users/me/Downloads/a.zip" },
        panes,
      ),
    ).toBe("/Users/me/Downloads");
  });

  test("rejects remote panes, unknown panes and empty space", () => {
    expect(resolveDestination({ kind: "pane", paneId: "remote" }, panes)).toBeNull();
    expect(resolveDestination({ kind: "pane", paneId: "gone" }, panes)).toBeNull();
    expect(resolveDestination(null, panes)).toBeNull();
  });
});
