import { EMPTY_SELECTION, type Selection } from "../selection/selection.js";

export type Location = { name: string; path: string; kind: string };
export type ViewMode = "list" | "grid";

export type Tab = {
  id: string;
  location: Location;
  back: Location[];
  forward: Location[];
  viewMode: ViewMode;
  selection: Selection;
};

export type TabsState = { tabs: Tab[]; activeId: string };

export const OVERVIEW: Location = { name: "Overview", path: "", kind: "overview" };

function newTab(id: string, location: Location, viewMode: ViewMode): Tab {
  return { id, location, back: [], forward: [], viewMode, selection: EMPTY_SELECTION };
}

export function createTabsState(id: string, location: Location = OVERVIEW): TabsState {
  return { tabs: [newTab(id, location, "list")], activeId: id };
}

export function activeTab(state: TabsState): Tab {
  return state.tabs.find((tab) => tab.id === state.activeId) ?? state.tabs[0];
}

export function sameLocation(a: Location, b: Location): boolean {
  return a.name === b.name && a.path === b.path && a.kind === b.kind;
}

function replaceActive(state: TabsState, update: (tab: Tab) => Tab): TabsState {
  const active = activeTab(state);
  return { ...state, tabs: state.tabs.map((tab) => (tab.id === active.id ? update(tab) : tab)) };
}

export function openTab(state: TabsState, id: string, location?: Location): TabsState {
  const active = activeTab(state);
  const tabs = [...state.tabs];
  tabs.splice(
    tabs.indexOf(active) + 1,
    0,
    newTab(id, location ?? active.location, active.viewMode),
  );
  return { tabs, activeId: id };
}

export function navigate(state: TabsState, location: Location): TabsState {
  if (sameLocation(activeTab(state).location, location)) return state;
  return replaceActive(state, (tab) => ({
    ...tab,
    location,
    back: [...tab.back, tab.location],
    forward: [],
    selection: EMPTY_SELECTION,
  }));
}

export function goBack(state: TabsState): TabsState {
  const previous = activeTab(state).back.at(-1);
  if (!previous) return state;
  return replaceActive(state, (tab) => ({
    ...tab,
    location: previous,
    back: tab.back.slice(0, -1),
    forward: [tab.location, ...tab.forward],
    selection: EMPTY_SELECTION,
  }));
}

export function goForward(state: TabsState): TabsState {
  const next = activeTab(state).forward[0];
  if (!next) return state;
  return replaceActive(state, (tab) => ({
    ...tab,
    location: next,
    back: [...tab.back, tab.location],
    forward: tab.forward.slice(1),
    selection: EMPTY_SELECTION,
  }));
}

export function selectTab(state: TabsState, id: string): TabsState {
  if (id === state.activeId || !state.tabs.some((tab) => tab.id === id)) return state;
  return { ...state, activeId: id };
}

export function closeTab(state: TabsState, id: string): { state: TabsState; closeWindow: boolean } {
  const index = state.tabs.findIndex((tab) => tab.id === id);
  if (index < 0) return { state, closeWindow: false };
  if (state.tabs.length === 1) {
    const tab = state.tabs[0];
    if (tab.location.kind === OVERVIEW.kind) return { state, closeWindow: true };
    const reset: Tab = { ...tab, location: OVERVIEW, back: [], forward: [], selection: EMPTY_SELECTION };
    return { state: { tabs: [reset], activeId: reset.id }, closeWindow: false };
  }
  const tabs = state.tabs.filter((tab) => tab.id !== id);
  const activeId =
    id === state.activeId ? tabs[Math.min(index, tabs.length - 1)].id : state.activeId;
  return { state: { tabs, activeId }, closeWindow: false };
}

export function moveTab(state: TabsState, from: number, to: number): TabsState {
  const count = state.tabs.length;
  if (from === to || from < 0 || to < 0 || from >= count || to >= count) return state;
  const tabs = [...state.tabs];
  const [moved] = tabs.splice(from, 1);
  tabs.splice(to, 0, moved);
  return { ...state, tabs };
}

export function cycleTab(state: TabsState, delta: 1 | -1): TabsState {
  const count = state.tabs.length;
  const index = state.tabs.indexOf(activeTab(state));
  return selectTab(state, state.tabs[(index + delta + count) % count].id);
}

/** Cmd+1..8 select that position; Cmd+9 selects the last tab (Safari/Chrome convention). */
export function tabIndexFromDigit(digit: number, count: number): number | null {
  if (count === 0 || !Number.isInteger(digit) || digit < 1 || digit > 9) return null;
  if (digit === 9) return count - 1;
  return digit - 1 < count ? digit - 1 : null;
}

export function updateActive(
  state: TabsState,
  patch: Partial<Pick<Tab, "viewMode" | "selection">>,
): TabsState {
  return replaceActive(state, (tab) => ({ ...tab, ...patch }));
}
