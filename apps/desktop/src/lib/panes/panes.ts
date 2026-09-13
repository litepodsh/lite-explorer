import {
  OVERVIEW,
  activeTab,
  createTabsState,
  moveTab,
  type Location,
  type TabsState,
} from "../tabs/tabs.js";

export type PaneLayout = "row" | "column";

export type Pane = { id: string; tabs: TabsState };

export type PanesState = {
  panes: Pane[];
  activeId: string;
  layout: PaneLayout;
};

export function createPanesState(
  paneId: string,
  tabId: string,
  location: Location = OVERVIEW,
): PanesState {
  return {
    panes: [{ id: paneId, tabs: createTabsState(tabId, location) }],
    activeId: paneId,
    layout: "row",
  };
}

export function activePane(state: PanesState): Pane {
  return state.panes.find((pane) => pane.id === state.activeId) ?? state.panes[0];
}

export function paneFor(state: PanesState, id: string): Pane | null {
  return state.panes.find((pane) => pane.id === id) ?? null;
}

export function otherPane(state: PanesState): Pane | null {
  if (state.panes.length < 2) return null;
  return state.panes.find((pane) => pane.id !== state.activeId) ?? null;
}

export function setActive(state: PanesState, id: string): PanesState {
  if (id === state.activeId || !paneFor(state, id)) return state;
  return { ...state, activeId: id };
}

export function setLayout(state: PanesState, layout: PaneLayout): PanesState {
  if (layout === state.layout) return state;
  return { ...state, layout };
}

export function addPane(
  state: PanesState,
  paneId: string,
  tabId: string,
  focus = true,
): PanesState {
  if (state.panes.length >= 2) return state;
  const location = activeTab(activePane(state).tabs).location;
  return {
    ...state,
    panes: [...state.panes, { id: paneId, tabs: createTabsState(tabId, location) }],
    activeId: focus ? paneId : state.activeId,
  };
}

export function removePane(state: PanesState, id = state.panes[1]?.id): PanesState {
  if (state.panes.length < 2) return state;
  const pane = state.panes.find((pane) => pane.id !== id) ?? state.panes[0];
  return { ...state, panes: [pane], activeId: pane.id };
}

function replaceTabs(state: PanesState, paneId: string, tabs: TabsState): PanesState {
  return {
    ...state,
    panes: state.panes.map((pane) => (pane.id === paneId ? { ...pane, tabs } : pane)),
  };
}

export function moveTabBetweenPanes(
  state: PanesState,
  fromPaneId: string,
  toPaneId: string,
  fromIndex: number,
  toIndex: number,
  resetTabId: string,
): PanesState {
  const from = paneFor(state, fromPaneId);
  const to = paneFor(state, toPaneId);
  if (!from || !to) return state;
  if (fromPaneId === toPaneId) {
    return replaceTabs(state, fromPaneId, moveTab(from.tabs, fromIndex, toIndex));
  }

  const tabs = from.tabs.tabs;
  if (fromIndex < 0 || fromIndex >= tabs.length) return state;
  const moved = tabs[fromIndex];

  const targetTabs = [...to.tabs.tabs];
  const insertAt = Math.max(0, Math.min(toIndex, targetTabs.length));
  targetTabs.splice(insertAt, 0, moved);

  let sourceTabs: TabsState;
  if (tabs.length === 1) {
    sourceTabs = createTabsState(resetTabId, OVERVIEW);
  } else {
    const remaining = tabs.filter((tab) => tab.id !== moved.id);
    const activeId =
      from.tabs.activeId === moved.id
        ? remaining[Math.min(fromIndex, remaining.length - 1)].id
        : from.tabs.activeId;
    sourceTabs = { tabs: remaining, activeId };
  }

  const withSource = replaceTabs(state, from.id, sourceTabs);
  return replaceTabs(withSource, to.id, { tabs: targetTabs, activeId: moved.id });
}
