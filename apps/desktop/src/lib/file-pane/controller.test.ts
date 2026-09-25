import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { FilePaneController } from "./controller.svelte.js";
import { TabsStore } from "$lib/tabs/tabs.svelte.js";
import { OVERVIEW } from "$lib/tabs/tabs.js";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), Channel: class {} }));
vi.mock("$lib/settings/settings.svelte.js", () => ({
  settings: { current: { previewOpenByDefault: false, automaticSizesInHome: false } },
}));

function pane(name = "files") {
  const tabs = new TabsStore();
  tabs.navigate({ name, path: `/tmp/${name}`, kind: "folder" });
  return new FilePaneController("pane", tabs, () => {});
}
const entry = { name: "file.txt", path: "/tmp/files/file.txt", is_directory: false, is_hidden: false };

beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe("navigation", () => {
  it.each(["Overview", "Recents"])("browses an ordinary folder named %s", (name) => {
    const controller = pane(name);
    controller.entries = [entry];
    expect(controller.isBrowsableFolder()).toBe(true);
    expect(controller.sourceEntries).toEqual([entry]);
    expect(controller.selectAllListed(true)).toBe(true);
    expect(controller.selection.paths).toEqual([entry.path]);
  });

  it("clears the previous folder while the next listing is pending", async () => {
    const controller = pane();
    controller.entries = [entry];
    let resolve!: (entries: typeof entry[]) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise((done) => { resolve = done; }));
    const loading = controller.loadLocation(controller.tabs.active.location);
    expect(controller.entries).toEqual([]);
    expect(controller.listing).toBe(true);
    controller.tabs.navigate(OVERVIEW);
    await controller.loadLocation(OVERVIEW);
    resolve([entry]);
    await loading;
    expect(controller.entries).toEqual([]);
  });

  it("ignores refresh errors from a folder already left", async () => {
    const controller = pane();
    let reject!: (error: Error) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise((_, fail) => { reject = fail; }));
    const refresh = controller.refreshListing(controller.listingPath);
    controller.tabs.navigate(OVERVIEW);
    await controller.loadLocation(OVERVIEW);
    reject(new Error("Old folder unavailable"));
    await refresh;
    expect(controller.listingError).toBe("");
  });

  it("reports a recents failure and exits the loading state", async () => {
    const controller = pane();
    const recents = { name: "Recents", path: "", kind: "recents" };
    controller.tabs.navigate(recents);
    vi.mocked(invoke).mockRejectedValue(new Error("Could not read recents"));
    await controller.loadLocation(recents);
    expect(controller.listing).toBe(false);
    expect(controller.listingError).toContain("Could not read recents");
  });
});
