import { afterEach, describe, expect, it, mock } from "bun:test";

var invoke: ReturnType<typeof mock>;
mock.module("@tauri-apps/api/core", () => {
  invoke = mock(() => {});
  return { invoke };
});

import { iconFor, MAX_MEMORY_ENTRIES, requestIcon, resetIconCache } from "./icon-cache.svelte.js";

afterEach(() => {
  resetIconCache();
  invoke.mockReset();
});

async function waitFor(assert: () => void): Promise<void> {
  for (let i = 0; i < 50; i++) {
    try {
      assert();
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  }
  assert();
}

describe("icon cache", () => {
  it("batches every request in a tick into one call", async () => {
    invoke.mockResolvedValue(["data:a", "data:b"]);
    requestIcon("/a");
    requestIcon("/b");

    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
    expect(invoke).toHaveBeenCalledWith("file_icons", { paths: ["/a", "/b"] });
    await waitFor(() => expect(iconFor("/b")).toBe("data:b"));
  });

  it("does not re-request queued or cached paths", async () => {
    invoke.mockResolvedValue([null]);
    requestIcon("/a");
    requestIcon("/a");

    await waitFor(() => expect(iconFor("/a")).toBeNull());
    requestIcon("/a");
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("caches missing icons so failures do not loop", async () => {
    invoke.mockRejectedValue(new Error("no icon"));
    requestIcon("/a");

    await waitFor(() => expect(iconFor("/a")).toBeNull());
    requestIcon("/a");
    await Promise.resolve();

    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("keeps only the most recently used entries", async () => {
    const paths = Array.from({ length: MAX_MEMORY_ENTRIES + 1 }, (_, index) => `/${index}`);
    invoke.mockResolvedValue(paths.map((path) => `data:${path}`));
    paths.forEach(requestIcon);

    await waitFor(() => expect(iconFor(paths.at(-1)!)).toBe(`data:${paths.at(-1)}`));
    expect(iconFor(paths[0])).toBeUndefined();
  });
});
