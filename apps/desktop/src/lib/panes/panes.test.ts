import { describe, expect, test } from "bun:test";
import { OVERVIEW, type Location } from "../tabs/tabs.js";
import {
  activePane,
  addPane,
  createPanesState,
  moveTabBetweenPanes,
  otherPane,
  removePane,
  setActive,
  setLayout,
  type PanesState,
} from "./panes.js";

const desktop: Location = { name: "Desktop", path: "/Users/me/Desktop", kind: "folder" };
const docs: Location = { name: "Documents", path: "/Users/me/Documents", kind: "folder" };

describe("createPanesState", () => {
  test("starts single-pane, row layout, with the given initial location", () => {
    const state = createPanesState("p1", "t1", desktop);
    expect(state.panes.length).toBe(1);
    expect(state.activeId).toBe("p1");
    expect(state.layout).toBe("row");
    expect(state.panes[0].tabs.tabs[0].location).toEqual(desktop);
    expect(state.panes[0].tabs.tabs[0].id).toBe("t1");
  });
});

describe("addPane", () => {
  test("clones the active pane location, starts with one tab, and becomes active", () => {
    let state = createPanesState("p1", "t1", desktop);
    state = addPane(state, "p2", "t2");
    expect(state.panes.length).toBe(2);
    expect(state.activeId).toBe("p2");
    expect(state.panes[1].tabs.tabs.length).toBe(1);
    expect(state.panes[1].tabs.tabs[0].location).toEqual(desktop);
    expect(activePane(state).id).toBe("p2");
    expect(otherPane(state)?.id).toBe("p1");
  });

  test("is a no-op when two panes already exist", () => {
    let state = createPanesState("p1", "t1");
    state = addPane(state, "p2", "t2");
    const same = addPane(state, "p3", "t3");
    expect(same).toBe(state);
  });

  test("focus=false adds the pane without changing the active one", () => {
    let state = createPanesState("p1", "t1", desktop);
    state = addPane(state, "p2", "t2", false);
    expect(state.panes.length).toBe(2);
    expect(state.activeId).toBe("p1");
    expect(activePane(state).id).toBe("p1");
    expect(state.panes[1].tabs.tabs.length).toBe(1);
    expect(state.panes[1].tabs.tabs[0].location).toEqual(desktop);
  });
});

describe("removePane", () => {
  test("keeps the first pane and re-activates it", () => {
    let state = createPanesState("p1", "t1");
    state = addPane(state, "p2", "t2");
    state = setActive(state, "p2");
    state = removePane(state);
    expect(state.panes.length).toBe(1);
    expect(state.activeId).toBe("p1");
    expect(otherPane(state)).toBeNull();
  });

  test("removes the requested pane", () => {
    let state = createPanesState("p1", "t1");
    state = addPane(state, "p2", "t2");
    const next = removePane(state, "p1");
    expect(next.panes.map((pane) => pane.id)).toEqual(["p2"]);
    expect(next.activeId).toBe("p2");
  });

  test("is a no-op with a single pane", () => {
    const state = createPanesState("p1", "t1");
    expect(removePane(state)).toBe(state);
  });
});

describe("setActive / setLayout", () => {
  test("setActive ignores unknown ids", () => {
    const state = createPanesState("p1", "t1");
    expect(setActive(state, "nope")).toBe(state);
  });

  test("setLayout changes the layout once", () => {
    const state = createPanesState("p1", "t1");
    const column = setLayout(state, "column");
    expect(column.layout).toBe("column");
    expect(setLayout(column, "column")).toBe(column);
  });
});

describe("moveTabBetweenPanes", () => {
  test("moves a tab to the target pane at the given index and activates it there", () => {
    let state = createPanesState("p1", "a1", desktop);
    state = addPane(state, "p2", "b1");
    // Give pane p1 two tabs.
    state = {
      ...state,
      panes: state.panes.map((p) =>
        p.id === "p1"
          ? {
              ...p,
              tabs: {
                tabs: [...p.tabs.tabs, { ...p.tabs.tabs[0], id: "a2", location: docs }],
                activeId: "a1",
              },
            }
          : p,
      ),
    };
    const next = moveTabBetweenPanes(state, "p1", "p2", 1, 0, "reset");
    const from = next.panes.find((p) => p.id === "p1")!;
    const to = next.panes.find((p) => p.id === "p2")!;
    expect(from.tabs.tabs.map((t) => t.id)).toEqual(["a1"]);
    expect(to.tabs.tabs.map((t) => t.id)).toEqual(["a2", "b1"]);
    expect(to.tabs.activeId).toBe("a2");
    expect(next.activeId).toBe("p2"); // global focus unchanged by the move itself
  });

  test("resets the source pane to Overview when it loses its only tab", () => {
    let state = createPanesState("p1", "a1", desktop);
    state = addPane(state, "p2", "b1");
    const next = moveTabBetweenPanes(state, "p1", "p2", 0, 0, "reset");
    const from = next.panes.find((p) => p.id === "p1")!;
    expect(from.tabs.tabs.length).toBe(1);
    expect(from.tabs.tabs[0].id).toBe("reset");
    expect(from.tabs.tabs[0].location).toEqual(OVERVIEW);
  });

  test("reorders within the same pane", () => {
    let state = createPanesState("p1", "a1", desktop);
    state = addPane(state, "p2", "b1");
    state = {
      ...state,
      panes: state.panes.map((p) =>
        p.id === "p1"
          ? {
              ...p,
              tabs: {
                tabs: [p.tabs.tabs[0], { ...p.tabs.tabs[0], id: "a2", location: docs }],
                activeId: "a1",
              },
            }
          : p,
      ),
    };
    const next: PanesState = moveTabBetweenPanes(state, "p1", "p1", 0, 1, "reset");
    expect(next.panes.find((p) => p.id === "p1")!.tabs.tabs.map((t) => t.id)).toEqual(["a2", "a1"]);
  });
});
