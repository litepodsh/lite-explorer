import { describe, expect, test } from "bun:test";
import {
  OVERVIEW,
  activeTab,
  closeTab,
  createTabsState,
  cycleTab,
  goBack,
  goForward,
  moveTab,
  navigate,
  openTab,
  selectTab,
  tabIndexFromDigit,
  updateActive,
  type Location,
  type TabsState,
} from "./tabs.js";

const desktop: Location = { name: "Desktop", path: "/Users/me/Desktop", kind: "folder" };
const documents: Location = { name: "Documents", path: "/Users/me/Documents", kind: "folder" };
const src: Location = { name: "src", path: "/Users/me/code/src", kind: "folder" };

function ids(state: TabsState): string[] {
  return state.tabs.map((tab) => tab.id);
}

describe("createTabsState", () => {
  test("starts with one Overview tab in list view", () => {
    const state = createTabsState("a");
    expect(ids(state)).toEqual(["a"]);
    expect(state.activeId).toBe("a");
    expect(activeTab(state).location).toEqual(OVERVIEW);
    expect(activeTab(state).viewMode).toBe("list");
    expect(activeTab(state).back).toEqual([]);
  });
});

describe("navigate", () => {
  test("pushes the current location to back and clears forward", () => {
    let state = navigate(createTabsState("a"), desktop);
    state = navigate(state, documents);
    state = goBack(state);
    expect(activeTab(state).forward).toEqual([documents]);
    state = navigate(state, src);
    expect(activeTab(state).location).toEqual(src);
    expect(activeTab(state).back).toEqual([OVERVIEW, desktop]);
    expect(activeTab(state).forward).toEqual([]);
  });

  test("clears the selection", () => {
    let state = navigate(createTabsState("a"), desktop);
    state = updateActive(state, { selectedEntryPath: "/Users/me/Desktop/a.txt" });
    state = navigate(state, documents);
    expect(activeTab(state).selectedEntryPath).toBe("");
  });

  test("is a no-op for the same location", () => {
    const state = navigate(createTabsState("a"), desktop);
    expect(navigate(state, { ...desktop })).toBe(state);
  });

  test("treats sidebar entries with the same kind but different names as different", () => {
    const red: Location = { name: "Red", path: "", kind: "tag" };
    const blue: Location = { name: "Blue", path: "", kind: "tag" };
    const state = navigate(navigate(createTabsState("a"), red), blue);
    expect(activeTab(state).location).toEqual(blue);
  });
});

describe("goBack / goForward", () => {
  test("round-trips through history", () => {
    let state = navigate(navigate(createTabsState("a"), desktop), documents);
    state = goBack(state);
    expect(activeTab(state).location).toEqual(desktop);
    expect(activeTab(state).back).toEqual([OVERVIEW]);
    expect(activeTab(state).forward).toEqual([documents]);
    state = goForward(state);
    expect(activeTab(state).location).toEqual(documents);
    expect(activeTab(state).back).toEqual([OVERVIEW, desktop]);
    expect(activeTab(state).forward).toEqual([]);
  });

  test("are no-ops on empty stacks", () => {
    const state = createTabsState("a");
    expect(goBack(state)).toBe(state);
    expect(goForward(state)).toBe(state);
  });
});

describe("openTab", () => {
  test("copies location and view mode, has empty history, inserts right of active, becomes active", () => {
    let state = navigate(createTabsState("a"), desktop);
    state = updateActive(state, { viewMode: "grid" });
    state = openTab(state, "b");
    state = selectTab(state, "a");
    state = openTab(state, "c");
    expect(ids(state)).toEqual(["a", "c", "b"]);
    expect(state.activeId).toBe("c");
    const tab = activeTab(state);
    expect(tab.location).toEqual(desktop);
    expect(tab.viewMode).toBe("grid");
    expect(tab.back).toEqual([]);
    expect(tab.forward).toEqual([]);
    expect(tab.selectedEntryPath).toBe("");
  });

  test("uses the given location when provided", () => {
    const state = openTab(navigate(createTabsState("a"), desktop), "b", src);
    expect(activeTab(state).location).toEqual(src);
  });

  test("history stays independent per tab", () => {
    let state = navigate(createTabsState("a"), desktop);
    state = openTab(state, "b");
    state = navigate(state, documents);
    state = selectTab(state, "a");
    expect(activeTab(state).location).toEqual(desktop);
    expect(activeTab(state).back).toEqual([OVERVIEW]);
  });
});

describe("selectTab", () => {
  test("ignores unknown ids", () => {
    const state = createTabsState("a");
    expect(selectTab(state, "zzz")).toBe(state);
  });
});

describe("closeTab", () => {
  function threeTabs(): TabsState {
    return openTab(openTab(createTabsState("a"), "b"), "c");
  }

  test("closing the active tab activates the right neighbor", () => {
    const start = selectTab(threeTabs(), "b");
    const { state, closeWindow } = closeTab(start, "b");
    expect(ids(state)).toEqual(["a", "c"]);
    expect(state.activeId).toBe("c");
    expect(closeWindow).toBe(false);
  });

  test("closing the active last-position tab activates the left neighbor", () => {
    const { state } = closeTab(threeTabs(), "c");
    expect(ids(state)).toEqual(["a", "b"]);
    expect(state.activeId).toBe("b");
  });

  test("closing an inactive tab keeps the active tab", () => {
    const { state } = closeTab(threeTabs(), "a");
    expect(ids(state)).toEqual(["b", "c"]);
    expect(state.activeId).toBe("c");
  });

  test("the only tab away from Overview resets to Overview with empty history", () => {
    const start = navigate(navigate(createTabsState("a"), desktop), documents);
    const { state, closeWindow } = closeTab(start, "a");
    expect(closeWindow).toBe(false);
    expect(ids(state)).toEqual(["a"]);
    expect(activeTab(state).location).toEqual(OVERVIEW);
    expect(activeTab(state).back).toEqual([]);
    expect(activeTab(state).forward).toEqual([]);
  });

  test("the only tab on Overview asks to close the window", () => {
    const start = createTabsState("a");
    const { state, closeWindow } = closeTab(start, "a");
    expect(closeWindow).toBe(true);
    expect(state).toBe(start);
  });

  test("unknown ids are ignored", () => {
    const start = threeTabs();
    const { state, closeWindow } = closeTab(start, "zzz");
    expect(state).toBe(start);
    expect(closeWindow).toBe(false);
  });
});

describe("moveTab", () => {
  test("reorders and keeps the active id", () => {
    const start = openTab(openTab(createTabsState("a"), "b"), "c");
    const state = moveTab(start, 2, 0);
    expect(ids(state)).toEqual(["c", "a", "b"]);
    expect(state.activeId).toBe("c");
  });

  test("ignores out-of-range and identical indexes", () => {
    const start = openTab(createTabsState("a"), "b");
    expect(moveTab(start, 0, 0)).toBe(start);
    expect(moveTab(start, -1, 1)).toBe(start);
    expect(moveTab(start, 0, 2)).toBe(start);
  });
});

describe("cycleTab", () => {
  test("wraps in both directions", () => {
    const start = openTab(openTab(createTabsState("a"), "b"), "c");
    expect(cycleTab(start, 1).activeId).toBe("a");
    expect(cycleTab(selectTab(start, "a"), -1).activeId).toBe("c");
    expect(cycleTab(selectTab(start, "a"), 1).activeId).toBe("b");
  });
});

describe("tabIndexFromDigit", () => {
  test("1..8 map to their index when the tab exists", () => {
    expect(tabIndexFromDigit(1, 3)).toBe(0);
    expect(tabIndexFromDigit(3, 3)).toBe(2);
    expect(tabIndexFromDigit(8, 12)).toBe(7);
  });

  test("9 maps to the last tab", () => {
    expect(tabIndexFromDigit(9, 3)).toBe(2);
    expect(tabIndexFromDigit(9, 12)).toBe(11);
  });

  test("out-of-range digits return null", () => {
    expect(tabIndexFromDigit(4, 3)).toBeNull();
    expect(tabIndexFromDigit(0, 3)).toBeNull();
    expect(tabIndexFromDigit(10, 3)).toBeNull();
    expect(tabIndexFromDigit(1, 0)).toBeNull();
  });
});
