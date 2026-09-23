import type { SortColumn, SortDir } from "../components/custom/file-list/sort.js";
import type { CreateKind } from "../file-ops/items.js";
import type { NavKey } from "../selection/selection.js";
import type { ClipboardMode } from "../transfer-clipboard/queue.js";
import type { Mode } from "./keymap.js";
import type { Scope } from "./scope.js";

export type NavModifiers = { shift: boolean; primary: boolean };
export type CopyTextKind = "path" | "dirPath" | "name" | "stem";
export type GoTarget = "home" | "downloads" | "overview" | "recents" | "shared";

/** Actions on the active pane's file list. Methods return false when the key did nothing. */
export type ListActions = {
  /** Renaming, a context menu or a confirmation is open. */
  blocked(): boolean;
  view(): "list" | "grid";
  remote(): boolean;
  moveFocus(key: NavKey, modifiers: NavModifiers): boolean;
  moveHalfPage(direction: 1 | -1): boolean;
  /** Jumps to the first entry whose name starts with the letters typed so far. */
  typeSelect(char: string): boolean;
  enterFocused(): boolean;
  enterOrOpenFocused(): boolean;
  openParent(): boolean;
  selectFocused(mode: "only" | "toggle"): boolean;
  toggleFocusedAndNext(): boolean;
  selectAll(): boolean;
  invertSelection(): boolean;
  startVisual(mode: "add" | "remove"): boolean;
  exitVisual(): boolean;
  clearSelection(): boolean;
  trashSelection(options: { permanent: boolean }): boolean;
  openSelection(): boolean;
  renameFocused(): boolean;
  createItem(kind: CreateKind): boolean;
  copyText(kind: CopyTextKind): boolean;
  enqueueSelected(mode: ClipboardMode): boolean;
  canPaste(): boolean;
  paste(): void;
  clearClipboard(): boolean;
  sort(column: SortColumn, dir: SortDir): boolean;
};

/** What commands can reach. Built by the page for every key press. */
export type AppContext = {
  scope: Scope;
  list: ListActions;
  panes: {
    activeIndex(): number;
    count(): number;
    activate(index: number): void;
    newTabInOtherPane(): void;
    toggleSecond(): void;
  };
  tabs: {
    selectDigit(digit: number): boolean;
    newTab(): void;
    cycle(delta: 1 | -1): void;
    moveActive(delta: 1 | -1): boolean;
    history(direction: "back" | "forward"): boolean;
  };
  checkboxes: { active(): boolean; exit(): void };
  preview: { toggle(): void; scroll(direction: 1 | -1): boolean };
  regions: {
    cycle(direction: 1 | -1): boolean;
    focus(region: "sidebar" | "list"): boolean;
    sidebarMove(delta: 1 | -1): boolean;
    sidebarOpen(): boolean;
  };
  app: {
    mode(): Mode;
    toggleKeyboardMode(): void;
    toggleHidden(): void;
    toggleActivity(): void;
    openShortcuts(): void;
    openSettings(): Promise<void>;
    setView(mode: "list" | "grid"): void;
  };
  go(target: GoTarget): void | Promise<void>;
  focusSearch(): void;
  searchContent(): void;
  togglePalette(): void;
  openPalette(query: string): void;
  currentPath(): string;
  /** Whether the focused element is inside `selector`. */
  focusInside(selector: string): boolean;
};
