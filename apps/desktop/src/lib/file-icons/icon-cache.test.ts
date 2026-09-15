import { afterEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { iconFor, requestIcon, resetIconCache } from "./icon-cache.svelte.js";

afterEach(() => {
  resetIconCache();
  invoke.mockReset();
});

describe("icon cache", () => {
  it("batches every request in a tick into one call", async () => {
    invoke.mockResolvedValue(["data:a", "data:b"]);
    requestIcon("/a");
    requestIcon("/b");

    await vi.waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    expect(invoke).toHaveBeenCalledWith("file_icons", { paths: ["/a", "/b"] });
    await vi.waitFor(() => expect(iconFor("/b")).toBe("data:b"));
  });

  it("does not re-request queued or cached paths", async () => {
    invoke.mockResolvedValue([null]);
    requestIcon("/a");
    requestIcon("/a");

    await vi.waitFor(() => expect(iconFor("/a")).toBeNull());
    requestIcon("/a");
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("caches missing icons so failures do not loop", async () => {
    invoke.mockRejectedValue(new Error("no icon"));
    requestIcon("/a");

    await vi.waitFor(() => expect(iconFor("/a")).toBeNull());
    requestIcon("/a");
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
