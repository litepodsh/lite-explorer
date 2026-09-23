import { acceleratorToAuthored, normalizeToken, type KeyPlatform } from "./keys.js";
import type { Scope } from "./scope.js";
import menuAccelerators from "./menu-accelerators.json";

export type Mode = "standard" | "yazi";
export type Category =
  | "Navigation"
  | "Selection"
  | "Files"
  | "Search"
  | "Sort"
  | "Tabs & Panes"
  | "Preview"
  | "App";

export type Binding = {
  /** One key, or two for a chord. Authored form: `Mod+Shift+P`, `Ctrl+d`, `<Space>`, `g`. */
  keys: string[];
  scope: Scope;
  mode: Mode;
  desc: string;
  category: Category;
  /** Command id; absent for bindings the menu or a component runs. */
  command?: string;
  /** Menu item this binding stands for (`menu-accelerators.json`). */
  menuId?: string;
  /** `menu` and `component` bindings are only listed; the menu or the component runs them. */
  source?: "app" | "menu" | "component";
  args?: Record<string, unknown>;
  /** Defaults to true. False ignores held-down repeats. */
  repeat?: boolean;
  /** A yazi binding that replaces the standard binding with the same scope and keys. */
  overrides?: boolean;
  /** Modifiers the key also accepts; the command receives `shift` and `primary` args. */
  modifiers?: ("Shift" | "Mod")[];
  /** Restricted scopes where this global binding still works. */
  where?: ("input" | "dialog" | "monaco")[];
  platforms?: KeyPlatform[];
};

export const MENU_ACCELERATORS: { id: string; accelerator: string }[] = menuAccelerators;

export const MENU_META: Record<string, { desc: string; category: Category }> = {
  "toggle-hidden-files": { desc: "Show or hide hidden files", category: "App" },
  "new-folder": { desc: "New folder", category: "Files" },
  "new-file": { desc: "New file", category: "Files" },
  "go-to-folder": { desc: "Command palette", category: "App" },
  "new-tab": { desc: "New tab", category: "Tabs & Panes" },
  "close-tab": { desc: "Close tab", category: "Tabs & Panes" },
  "next-tab": { desc: "Next tab", category: "Tabs & Panes" },
  "previous-tab": { desc: "Previous tab", category: "Tabs & Panes" },
  "new-tab-other": { desc: "New tab in other pane", category: "Tabs & Panes" },
  "toggle-second-pane": { desc: "Show or hide second pane", category: "Tabs & Panes" },
  "open-settings": { desc: "Settings", category: "App" },
};

const TEXT_FIELDS: Binding["where"] = ["input", "monaco"];

function nav(key: string, command: string, desc: string): Binding {
  return {
    keys: [key],
    command,
    scope: "list",
    mode: "standard",
    desc,
    category: "Navigation",
    modifiers: ["Shift", "Mod"],
  };
}

function standard(
  keys: string,
  command: string,
  desc: string,
  category: Category,
  extra: Partial<Binding> = {},
): Binding {
  return { keys: [keys], command, scope: "list", mode: "standard", desc, category, ...extra };
}

export const STANDARD_BINDINGS: Binding[] = [
  nav("<Up>", "list.prev", "Previous item"),
  nav("<Down>", "list.next", "Next item"),
  nav("<PageUp>", "list.pageUp", "Up one page"),
  nav("<PageDown>", "list.pageDown", "Down one page"),
  nav("<Home>", "list.top", "First item"),
  nav("<End>", "list.bottom", "Last item"),
  nav("<Left>", "list.left", "Parent folder in list view, previous item in icon view"),
  nav("<Right>", "list.right", "Open folder in list view, next item in icon view"),
  standard("<Enter>", "file.open", "Open selection", "Files", { repeat: false }),
  standard("<Space>", "selection.only", "Select focused item", "Selection"),
  standard("Mod+<Space>", "selection.toggle", "Toggle focused item", "Selection"),
  standard("Mod+A", "selection.all", "Select all", "Selection"),
  standard("<Esc>", "app.escape", "Clear selection or leave checkbox mode", "Selection", {
    scope: "global",
  }),
  standard("<Delete>", "file.trash", "Move to Trash", "Files", { repeat: false }),
  standard("<Backspace>", "file.trash", "Move to Trash", "Files", { repeat: false }),
  ...[
    "Shift+<Delete>",
    "Shift+<Backspace>",
    "Meta+<Delete>",
    "Meta+<Backspace>",
    "Shift+Meta+<Delete>",
    "Shift+Meta+<Backspace>",
  ].map((keys) =>
    standard(keys, "file.deletePermanent", "Delete permanently", "Files", { repeat: false }),
  ),
  standard("Mod+C", "file.copy", "Copy to transfer clipboard", "Files", {
    scope: "global",
    repeat: false,
  }),
  standard("Mod+X", "file.cut", "Cut to transfer clipboard", "Files", {
    scope: "global",
    repeat: false,
  }),
  standard("Mod+V", "file.paste", "Paste here", "Files", { scope: "global", repeat: false }),
  standard("Mod+F", "search.focus", "Search this folder", "Search", {
    scope: "global",
    where: ["input"],
  }),
  standard("Mod+Shift+P", "palette.open", "Command palette", "App", {
    scope: "global",
    where: ["input", "dialog", "monaco"],
    menuId: "go-to-folder",
    repeat: false,
  }),
  standard("Mod+Shift+T", "tab.newInOtherPane", "New tab in other pane", "Tabs & Panes", {
    scope: "global",
    where: TEXT_FIELDS,
    menuId: "new-tab-other",
    repeat: false,
  }),
  standard("Mod+<Left>", "pane.left", "Focus left pane", "Tabs & Panes", {
    scope: "global",
    where: TEXT_FIELDS,
  }),
  standard("Mod+<Right>", "pane.right", "Focus right pane", "Tabs & Panes", {
    scope: "global",
    where: TEXT_FIELDS,
  }),
  ...Array.from({ length: 9 }, (_, index) =>
    standard(
      `Mod+${index + 1}`,
      "tab.goto",
      index === 8 ? "Go to last tab" : `Go to tab ${index + 1}`,
      "Tabs & Panes",
      {
        scope: "global",
        where: TEXT_FIELDS,
        args: { digit: index + 1 },
      },
    ),
  ),
];

function binding(
  keys: string[],
  command: string,
  desc: string,
  category: Category,
  extra: Partial<Binding> = {},
): Binding {
  return { keys, command, scope: "list", mode: "standard", desc, category, ...extra };
}

const FIELDS_AND_CODE: Binding["where"] = ["input", "monaco"];

/** Standard bindings added for keyboard-only use. */
export const EXTENDED_BINDINGS: Binding[] = [
  binding(["Alt+<Up>"], "folder.parent", "Parent folder", "Navigation"),
  binding(["Mod+["], "history.back", "Back", "Navigation", { scope: "global", where: ["input"] }),
  binding(["Mod+]"], "history.forward", "Forward", "Navigation", {
    scope: "global",
    where: ["input"],
  }),
  binding(["<F2>"], "file.rename", "Rename", "Files", { repeat: false }),
  binding(["Mod+Alt+C"], "file.copyPath", "Copy path", "Files", { repeat: false }),
  binding(["Mod+L"], "palette.goToPath", "Go to path", "Search", {
    scope: "global",
    where: FIELDS_AND_CODE,
    repeat: false,
  }),
  binding(["Mod+Alt+P"], "preview.toggle", "Show or hide preview", "Preview", {
    scope: "global",
    repeat: false,
  }),
  binding(["<F1>"], "app.shortcuts", "Keyboard shortcuts", "App", {
    scope: "global",
    where: ["input"],
    repeat: false,
  }),
  binding(["Mod+/"], "app.shortcuts", "Keyboard shortcuts", "App", {
    scope: "global",
    where: ["input"],
    repeat: false,
  }),
  binding(["<F6>"], "region.next", "Next region", "Navigation", {
    scope: "global",
    where: FIELDS_AND_CODE,
  }),
  binding(["Shift+<F6>"], "region.prev", "Previous region", "Navigation", {
    scope: "global",
    where: FIELDS_AND_CODE,
  }),
  binding(["Mod+Shift+E"], "region.sidebar", "Focus sidebar", "Navigation", {
    scope: "global",
    where: FIELDS_AND_CODE,
  }),
  binding(["<Up>"], "sidebar.prev", "Previous sidebar item", "Navigation", { scope: "sidebar" }),
  binding(["<Down>"], "sidebar.next", "Next sidebar item", "Navigation", { scope: "sidebar" }),
  binding(["<Esc>"], "region.list", "Back to the file list", "Navigation", { scope: "sidebar" }),
  binding(["<Esc>"], "region.list", "Back to the file list", "Navigation", { scope: "preview" }),
  binding(["<Esc>"], "region.list", "Back to the file list", "Navigation", { scope: "monaco" }),
];

function yazi(
  keys: string,
  command: string,
  desc: string,
  category: Category,
  extra: Partial<Binding> = {},
): Binding {
  return { keys: keys.split(" "), command, scope: "list", mode: "yazi", desc, category, ...extra };
}

const sortBindings = (["name", "size", "date", "type"] as const).flatMap((column) => {
  const key = { name: "n", size: "s", date: "m", type: "e" }[column];
  return [
    yazi(`, ${key}`, `sort.${column}`, `Sort by ${column}`, "Sort", { args: { dir: "asc" } }),
    yazi(`, ${key.toUpperCase()}`, `sort.${column}`, `Sort by ${column}, descending`, "Sort", {
      args: { dir: "desc" },
    }),
  ];
});

export const YAZI_BINDINGS: Binding[] = [
  yazi("j", "list.next", "Next item", "Navigation"),
  yazi("k", "list.prev", "Previous item", "Navigation"),
  yazi("g g", "list.top", "First item", "Navigation"),
  yazi("G", "list.bottom", "Last item", "Navigation"),
  yazi("Ctrl+d", "list.halfPageDown", "Down half a page", "Navigation"),
  yazi("Ctrl+u", "list.halfPageUp", "Up half a page", "Navigation"),
  yazi("l", "folder.enter", "Open focused folder or file", "Navigation"),
  yazi("h", "folder.parent", "Parent folder", "Navigation"),
  yazi("H", "history.back", "Back", "Navigation", { scope: "global" }),
  yazi("L", "history.forward", "Forward", "Navigation", { scope: "global" }),
  yazi("<Space>", "selection.toggleNext", "Toggle and move down", "Selection", { overrides: true }),
  yazi("Ctrl+a", "selection.all", "Select all", "Selection", { platforms: ["macos"] }),
  yazi("Ctrl+r", "selection.invert", "Invert selection", "Selection"),
  yazi("v", "selection.visual", "Visual mode: select a range", "Selection", {
    args: { mode: "add" },
    repeat: false,
  }),
  yazi("V", "selection.visual", "Visual mode: deselect a range", "Selection", {
    args: { mode: "remove" },
    repeat: false,
  }),
  yazi("o", "file.open", "Open selection", "Files", { repeat: false }),
  yazi("y", "file.copy", "Copy to transfer clipboard", "Files", { repeat: false }),
  yazi("x", "file.cut", "Cut to transfer clipboard", "Files", { repeat: false }),
  yazi("p", "file.paste", "Paste here", "Files", { repeat: false }),
  yazi("Y", "clipboard.clear", "Clear transfer clipboard", "Files", { repeat: false }),
  yazi("X", "clipboard.clear", "Clear transfer clipboard", "Files", { repeat: false }),
  yazi("d", "file.trash", "Move to Trash", "Files", { repeat: false }),
  yazi("D", "file.deletePermanent", "Delete permanently", "Files", { repeat: false }),
  yazi("a", "file.newFile", "New file", "Files", { repeat: false }),
  yazi("A", "file.newFolder", "New folder", "Files", { repeat: false }),
  yazi("r", "file.rename", "Rename", "Files", { repeat: false }),
  yazi("c c", "file.copyPath", "Copy path", "Files"),
  yazi("c d", "file.copyDirPath", "Copy folder path", "Files"),
  yazi("c f", "file.copyName", "Copy name", "Files"),
  yazi("c n", "file.copyStem", "Copy name without extension", "Files"),
  yazi(".", "view.toggleHidden", "Show or hide hidden files", "App", {
    scope: "global",
    repeat: false,
  }),
  yazi("s", "search.focus", "Search this folder", "Search", { scope: "global" }),
  yazi("/", "search.focus", "Search this folder", "Search", { scope: "global" }),
  yazi("S", "search.content", "Search file contents", "Search", { scope: "global" }),
  yazi("z", "palette.open", "Command palette", "Search", { scope: "global", repeat: false }),
  yazi("g <Space>", "palette.goToPath", "Go to path", "Search", { scope: "global" }),
  ...sortBindings,
  yazi("g h", "go.home", "Go home", "Navigation", { scope: "global" }),
  yazi("g d", "go.downloads", "Go to Downloads", "Navigation", { scope: "global" }),
  yazi("g o", "go.overview", "Go to Overview", "Navigation", { scope: "global" }),
  yazi("g r", "go.recents", "Go to Recents", "Navigation", { scope: "global" }),
  yazi("g s", "go.shared", "Go to Shared", "Navigation", { scope: "global" }),
  yazi("t t", "tab.new", "New tab here", "Tabs & Panes", { scope: "global" }),
  ...Array.from({ length: 9 }, (_, index) =>
    yazi(
      `${index + 1}`,
      "tab.goto",
      index === 8 ? "Go to last tab" : `Go to tab ${index + 1}`,
      "Tabs & Panes",
      {
        scope: "global",
        args: { digit: index + 1 },
      },
    ),
  ),
  yazi("[", "tab.prev", "Previous tab", "Tabs & Panes", { scope: "global" }),
  yazi("]", "tab.next", "Next tab", "Tabs & Panes", { scope: "global" }),
  yazi("{", "tab.moveLeft", "Move tab left", "Tabs & Panes", { scope: "global" }),
  yazi("}", "tab.moveRight", "Move tab right", "Tabs & Panes", { scope: "global" }),
  yazi("Ctrl+h", "pane.left", "Focus left pane", "Tabs & Panes", { scope: "global" }),
  yazi("Ctrl+l", "pane.right", "Focus right pane", "Tabs & Panes", {
    scope: "global",
    platforms: ["macos"],
  }),
  yazi("Ctrl+l", "pane.right", "Focus right pane", "Tabs & Panes", {
    scope: "global",
    platforms: ["windows", "linux"],
    overrides: true,
  }),
  yazi("\\", "pane.toggleSecond", "Show or hide second pane", "Tabs & Panes", {
    scope: "global",
    repeat: false,
  }),
  yazi("J", "preview.scrollDown", "Scroll preview down", "Preview"),
  yazi("K", "preview.scrollUp", "Scroll preview up", "Preview"),
  yazi("i", "preview.toggle", "Show or hide preview", "Preview", {
    scope: "global",
    repeat: false,
  }),
  yazi("w", "app.activity", "Show or hide activity", "App", { scope: "global", repeat: false }),
  yazi("?", "app.shortcuts", "Keyboard shortcuts", "App", { scope: "global", repeat: false }),
  yazi("~", "app.shortcuts", "Keyboard shortcuts", "App", { scope: "global", repeat: false }),
  yazi("j", "sidebar.next", "Next sidebar item", "Navigation", { scope: "sidebar" }),
  yazi("k", "sidebar.prev", "Previous sidebar item", "Navigation", { scope: "sidebar" }),
  yazi("l", "sidebar.open", "Open sidebar item", "Navigation", { scope: "sidebar", repeat: false }),
  yazi("h", "region.list", "Back to the file list", "Navigation", { scope: "sidebar" }),
  yazi("j", "preview.scrollDown", "Scroll preview down", "Preview", { scope: "preview" }),
  yazi("k", "preview.scrollUp", "Scroll preview up", "Preview", { scope: "preview" }),
  yazi("h", "region.list", "Back to the file list", "Navigation", { scope: "preview" }),
];

/** Shortcuts components handle themselves; listed in the dialog only. */
export const COMPONENT_BINDINGS: Binding[] = [
  {
    keys: ["<F5>"],
    scope: "global",
    mode: "standard",
    source: "component",
    desc: "Refresh current folder",
    category: "Navigation",
  },
  {
    keys: ["Mod+F"],
    scope: "preview",
    mode: "standard",
    source: "component",
    desc: "Find in code or archive preview",
    category: "Preview",
  },
  {
    keys: ["Mod+="],
    scope: "preview",
    mode: "standard",
    source: "component",
    desc: "Larger preview text",
    category: "Preview",
  },
  {
    keys: ["Mod+-"],
    scope: "preview",
    mode: "standard",
    source: "component",
    desc: "Smaller preview text",
    category: "Preview",
  },
  {
    keys: ["Mod+0"],
    scope: "preview",
    mode: "standard",
    source: "component",
    desc: "Default preview text size",
    category: "Preview",
  },
];

export const APP_BINDINGS: Binding[] = [
  ...STANDARD_BINDINGS,
  ...EXTENDED_BINDINGS,
  ...YAZI_BINDINGS,
];

/** Bindings that list menu accelerators no app binding already runs. */
export function menuBindings(appBindings: Binding[]): Binding[] {
  const covered = new Set(appBindings.flatMap((item) => (item.menuId ? [item.menuId] : [])));
  return MENU_ACCELERATORS.filter(({ id }) => !covered.has(id)).map(
    ({ id, accelerator }): Binding => {
      const authored = acceleratorToAuthored(accelerator);
      const meta = MENU_META[id];
      if (!authored || !meta) throw new Error(`Menu accelerator “${id}” has no keymap entry`);
      return {
        keys: [authored],
        menuId: id,
        source: "menu",
        scope: "global",
        mode: "standard",
        desc: meta.desc,
        category: meta.category,
      };
    },
  );
}

export function allBindings(): Binding[] {
  return [...APP_BINDINGS, ...COMPONENT_BINDINGS, ...menuBindings(APP_BINDINGS)];
}

export function appliesTo(binding: Binding, platform: KeyPlatform): boolean {
  return !binding.platforms || binding.platforms.includes(platform);
}

/** Scopes whose bindings a key in `scope` can reach. */
function reachable(scope: Scope): Scope[] {
  return scope === "global" ? ["global"] : [scope, "global"];
}

/** Human-readable keymap problems; empty when the keymap is consistent. */
export function findConflicts(bindings: Binding[], platform: KeyPlatform): string[] {
  const problems: string[] = [];
  const active = bindings.filter((item) => appliesTo(item, platform));
  const tokens = (item: Binding) => item.keys.map((key) => normalizeToken(key, platform));
  const label = (item: Binding) => tokens(item).join(" ");

  for (const mode of ["standard", "yazi"] as const) {
    const inMode = active.filter((item) => item.mode === mode);
    const scopes = [...new Set(inMode.map((item) => item.scope))];
    for (const scope of scopes) {
      const own = inMode.filter((item) => item.scope === scope);
      const seen = new Set<string>();
      for (const item of own) {
        const key = label(item);
        if (seen.has(key)) problems.push(`${mode} ${scope}: “${key}” is bound twice`);
        seen.add(key);
      }
      const visible = inMode.filter((item) => reachable(scope).includes(item.scope));
      const prefixes = new Set(
        visible.filter((item) => item.keys.length > 1).map((item) => tokens(item)[0]),
      );
      for (const item of own) {
        if (item.keys.length === 1 && prefixes.has(tokens(item)[0])) {
          problems.push(`${mode} ${scope}: “${label(item)}” is also the start of a chord`);
        }
      }
    }
  }

  const standardKeys = new Set(
    active.filter((item) => item.mode === "standard").map((item) => `${item.scope} ${label(item)}`),
  );
  for (const item of active.filter((candidate) => candidate.mode === "yazi")) {
    const clash = standardKeys.has(`${item.scope} ${label(item)}`);
    if (clash && !item.overrides)
      problems.push(
        `yazi ${item.scope}: “${label(item)}” replaces a standard binding without overrides`,
      );
    if (!clash && item.overrides)
      problems.push(`yazi ${item.scope}: “${label(item)}” overrides nothing`);
  }
  return problems;
}
