import type { AppContext, ListActions } from "./context.js";

export type ContextOverrides = Partial<Omit<AppContext, "list">> & { list?: Partial<ListActions> };

function record(calls: string[], call: string): true {
  calls.push(call);
  return true;
}

/** An `AppContext` whose actions log to `calls` and report success. For tests. */
export function fakeContext(calls: string[], overrides: ContextOverrides = {}): AppContext {
  const { list, ...rest } = overrides;
  return {
    scope: "list",
    list: {
      blocked: () => false,
      view: () => "list",
      remote: () => false,
      moveFocus: (key, modifiers) =>
        record(calls, `move ${key} ${modifiers.shift} ${modifiers.primary}`),
      moveHalfPage: (direction) => record(calls, `half ${direction}`),
      typeSelect: (char) => record(calls, `type ${char}`),
      enterFocused: () => record(calls, "enter"),
      enterOrOpenFocused: () => record(calls, "enter or open"),
      openParent: () => record(calls, "parent"),
      selectFocused: (mode) => record(calls, `select ${mode}`),
      toggleFocusedAndNext: () => record(calls, "toggle next"),
      selectAll: () => record(calls, "all"),
      invertSelection: () => record(calls, "invert"),
      startVisual: (mode) => record(calls, `visual ${mode}`),
      exitVisual: () => false,
      clearSelection: () => record(calls, "clear"),
      trashSelection: ({ permanent }) => record(calls, `trash ${permanent}`),
      openSelection: () => record(calls, "open"),
      renameFocused: () => record(calls, "rename"),
      createItem: (kind) => record(calls, `create ${kind}`),
      copyText: (kind) => record(calls, `copy ${kind}`),
      enqueueSelected: (mode) => record(calls, `enqueue ${mode}`),
      canPaste: () => true,
      paste: () => void calls.push("paste"),
      clearClipboard: () => record(calls, "clear clipboard"),
      sort: (column, dir) => record(calls, `sort ${column} ${dir}`),
      ...list,
    },
    panes: {
      activeIndex: () => 0,
      count: () => 2,
      activate: (index) => void calls.push(`pane ${index}`),
      newTabInOtherPane: () => void calls.push("tab other"),
      toggleSecond: () => void calls.push("toggle pane"),
    },
    tabs: {
      selectDigit: (digit) => record(calls, `tab ${digit}`) && digit <= 3,
      newTab: () => void calls.push("new tab"),
      cycle: (delta) => void calls.push(`cycle ${delta}`),
      moveActive: (delta) => record(calls, `move tab ${delta}`),
      history: (direction) => record(calls, `history ${direction}`),
    },
    checkboxes: { active: () => false, exit: () => void calls.push("exit checkboxes") },
    preview: {
      toggle: () => void calls.push("toggle preview"),
      scroll: (direction) => record(calls, `scroll ${direction}`),
    },
    regions: {
      cycle: (direction) => record(calls, `region ${direction}`),
      focus: (region) => record(calls, `focus ${region}`),
      sidebarMove: (delta) => record(calls, `sidebar ${delta}`),
      sidebarOpen: () => record(calls, "sidebar open"),
    },
    app: {
      mode: () => "standard",
      toggleKeyboardMode: () => void calls.push("toggle mode"),
      toggleHidden: () => void calls.push("toggle hidden"),
      toggleActivity: () => void calls.push("toggle activity"),
      openShortcuts: () => void calls.push("shortcuts"),
      openSettings: async () => void calls.push("settings"),
      setView: (mode) => void calls.push(`view ${mode}`),
    },
    go: (target) => void calls.push(`go ${target}`),
    focusSearch: () => void calls.push("search"),
    searchContent: () => void calls.push("search content"),
    togglePalette: () => void calls.push("palette"),
    openPalette: (query) => void calls.push(`palette ${query}`),
    currentPath: () => "/Users/me",
    focusInside: () => false,
    ...rest,
  };
}
