import { afterEach, describe, expect, it, mock } from "bun:test";

var invoke: ReturnType<typeof mock>;
mock.module("@tauri-apps/api/core", () => {
  invoke = mock(() => Promise.resolve([]));
  return { invoke };
});

import {
  requestThumbnails,
  resetThumbnailCache,
  thumbnailFor,
} from "./thumbnail-cache.svelte.js";

afterEach(() => {
  resetThumbnailCache();
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

describe("thumbnail cache", () => {
  it("resolves disk-cached thumbnails immediately before queuing misses", async () => {
    invoke.mockImplementation((cmd: string, args: { paths?: string[]; cachedOnly?: boolean }) => {
      if (cmd === "image_thumbnails" && args.cachedOnly) {
        return Promise.resolve([{ path: "/folder-a/cached.jpg", url: "media://cached" }]);
      }
      if (cmd === "image_thumbnails" && !args.cachedOnly) {
        return Promise.resolve([{ path: "/folder-a/uncached.jpg", url: "media://generated" }]);
      }
      return Promise.resolve([]);
    });

    requestThumbnails(["/folder-a/cached.jpg", "/folder-a/uncached.jpg"], true, "/folder-a");

    await waitFor(() => {
      expect(thumbnailFor("/folder-a/cached.jpg")).toBe("media://cached");
      expect(thumbnailFor("/folder-a/uncached.jpg")).toBe("media://generated");
    });

    const generateCalls = invoke.mock.calls.filter(
      ([cmd, args]) => cmd === "image_thumbnails" && !args?.cachedOnly,
    );
    expect(generateCalls).toHaveLength(1);
    expect(generateCalls[0][1].paths).toEqual(["/folder-a/uncached.jpg"]);
  });

  it("cancels pending generation from previous folder when switching folders", async () => {
    let resolveOldBatch: ((items: { path: string; url: string }[]) => void) | undefined;
    invoke.mockImplementation((cmd: string, args: { paths?: string[]; cachedOnly?: boolean; generation?: number }) => {
      if (cmd === "cancel_thumbnails") return Promise.resolve();
      if (cmd === "image_thumbnails" && args.cachedOnly) {
        if (args.paths?.includes("/folder-b/fast.jpg")) {
          return Promise.resolve([{ path: "/folder-b/fast.jpg", url: "media://fast-b" }]);
        }
        return Promise.resolve([]);
      }
      if (cmd === "image_thumbnails" && !args.cachedOnly) {
        if (args.paths?.[0]?.startsWith("/folder-a/")) {
          return new Promise((resolve) => {
            resolveOldBatch = resolve;
          });
        }
        return Promise.resolve(
          (args.paths ?? []).map((p) => ({ path: p, url: `media://${p}` })),
        );
      }
      return Promise.resolve([]);
    });

    // Request many items in folder-a (more than 1 batch)
    const folderAPaths = Array.from({ length: 10 }, (_, i) => `/folder-a/${i}.jpg`);
    requestThumbnails(folderAPaths, true, "/folder-a");

    await waitFor(() => expect(resolveOldBatch).toBeDefined());

    // Switch to folder-b while folder-a is still generating
    requestThumbnails(["/folder-b/fast.jpg", "/folder-b/new.jpg"], true, "/folder-b");

    await waitFor(() => {
      expect(thumbnailFor("/folder-b/fast.jpg")).toBe("media://fast-b");
      expect(thumbnailFor("/folder-b/new.jpg")).toBe("media:///folder-b/new.jpg");
    });

    // Verify cancel_thumbnails was called on folder switch
    const cancelCalls = invoke.mock.calls.filter(([cmd]) => cmd === "cancel_thumbnails");
    expect(cancelCalls.length).toBeGreaterThanOrEqual(2);

    // When the aborted folder-a batch returns empty, it must NOT poison the cache with `null`
    resolveOldBatch?.([]);
    await new Promise((r) => setTimeout(r, 20));
    for (const p of folderAPaths) {
      expect(thumbnailFor(p)).toBeUndefined();
    }
  });
});
