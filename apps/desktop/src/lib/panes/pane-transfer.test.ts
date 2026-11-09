import { describe, expect, test } from "bun:test";
import { paneTransferBlocked, type PaneTransfer } from "./pane-transfer.js";

function transfer(overrides: Partial<PaneTransfer> = {}): PaneTransfer {
  return {
    mode: "copy",
    paths: ["/a/file.txt"],
    destination: "/b",
    destinationIsFolder: true,
    sourceRemote: false,
    targetRemote: false,
    ...overrides,
  };
}

describe("paneTransferBlocked", () => {
  test("allows a local copy or move to another folder", () => {
    expect(paneTransferBlocked(transfer())).toBeNull();
    expect(paneTransferBlocked(transfer({ mode: "move" }))).toBeNull();
  });

  test("needs a real folder in the other pane", () => {
    expect(paneTransferBlocked(transfer({ destinationIsFolder: false }))).toContain("Open a folder");
  });

  test("blocks the folder the items are already in, with or without a trailing slash", () => {
    expect(paneTransferBlocked(transfer({ destination: "/a" }))).toContain("same folder");
    expect(paneTransferBlocked(transfer({ destination: "/a/" }))).toContain("same folder");
  });

  test("checks each item, as search results can come from several folders", () => {
    const paths = ["/x/one.txt", "/b/two.txt"];
    expect(paneTransferBlocked(transfer({ paths }))).toContain("same folder");
  });

  test("blocks a folder going inside itself", () => {
    const paths = ["/a/folder"];
    expect(paneTransferBlocked(transfer({ paths, destination: "/a/folder" }))).toContain("itself");
    expect(paneTransferBlocked(transfer({ paths, destination: "/a/folder/sub" }))).toContain("itself");
    expect(paneTransferBlocked(transfer({ paths, destination: "/a/folder-2" }))).toBeNull();
  });

  test("handles Windows paths", () => {
    const paths = ["C:\\a\\folder"];
    expect(paneTransferBlocked(transfer({ paths, destination: "C:\\a" }))).toContain("same folder");
    expect(paneTransferBlocked(transfer({ paths, destination: "C:\\a\\folder\\sub" }))).toContain("itself");
  });

  test("remote locations can copy but not move", () => {
    const remote = { paths: ["s3://bucket/file.txt"], sourceRemote: true };
    expect(paneTransferBlocked(transfer(remote))).toBeNull();
    expect(paneTransferBlocked(transfer({ ...remote, mode: "move" }))).toContain("remote");
    expect(paneTransferBlocked(transfer({ mode: "move", targetRemote: true }))).toContain("remote");
  });
});
