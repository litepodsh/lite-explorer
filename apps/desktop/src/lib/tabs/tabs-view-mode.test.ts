import { describe, expect, test } from "bun:test";
import { activeTab, createTabsState, openTab, updateActive } from "./tabs.js";

describe("view mode of new tabs", () => {
  test("the first tab can start in icon view", () => {
    expect(activeTab(createTabsState("a", undefined, "grid")).viewMode).toBe("grid");
    expect(activeTab(createTabsState("a")).viewMode).toBe("list");
  });

  test("a new tab uses the given view mode", () => {
    const state = updateActive(createTabsState("a"), { viewMode: "grid" });
    expect(activeTab(openTab(state, "b", undefined, "list")).viewMode).toBe("list");
  });

  test("without a view mode a new tab copies the active tab", () => {
    const state = updateActive(createTabsState("a"), { viewMode: "grid" });
    expect(activeTab(openTab(state, "b")).viewMode).toBe("grid");
  });
});
