import type { NavKey } from "../selection/selection.js";
import type { Command } from "./commands.js";
import type { AppContext, NavModifiers } from "./context.js";

type AppCommand = Command<AppContext>;

function modifiers(args: Record<string, unknown>): NavModifiers {
  return { shift: args.shift === true, primary: args.primary === true };
}

function unmodified(args: Record<string, unknown>): boolean {
  return args.shift !== true && args.primary !== true;
}

/** Only absolute local paths (remote locations and virtual pages have none). */
function isLocalFolder(path: string): boolean {
  return path.startsWith("/");
}

function listCommand(id: string, title: string, run: AppCommand["run"]): AppCommand {
  return { id, title, when: (context) => !context.list.blocked(), run };
}

function move(id: string, title: string, key: NavKey): AppCommand {
  return listCommand(id, title, (context, args) => context.list.moveFocus(key, modifiers(args)));
}

function focusPane(context: AppContext, delta: 1 | -1): boolean {
  const target = context.panes.activeIndex() + delta;
  if (target < 0 || target >= context.panes.count()) return false;
  context.panes.activate(target);
  return true;
}

export function standardCommands(): AppCommand[] {
  return [
    move("list.prev", "Previous Item", "ArrowUp"),
    move("list.next", "Next Item", "ArrowDown"),
    move("list.pageUp", "Up One Page", "PageUp"),
    move("list.pageDown", "Down One Page", "PageDown"),
    move("list.top", "First Item", "Home"),
    move("list.bottom", "Last Item", "End"),
    listCommand("list.left", "Left", (context, args) => {
      if (context.list.view() === "grid")
        return context.list.moveFocus("ArrowLeft", modifiers(args));
      return unmodified(args) && context.list.openParent();
    }),
    listCommand("list.right", "Right", (context, args) => {
      if (context.list.view() === "grid")
        return context.list.moveFocus("ArrowRight", modifiers(args));
      return unmodified(args) && context.list.enterFocused();
    }),
    listCommand("file.open", "Open", (context) => context.list.openSelection()),
    listCommand("selection.only", "Select Focused Item", (context) =>
      context.list.selectFocused("only"),
    ),
    listCommand("selection.toggle", "Toggle Focused Item", (context) =>
      context.list.selectFocused("toggle"),
    ),
    listCommand("selection.all", "Select All", (context) => context.list.selectAll()),
    listCommand("file.trash", "Move to Trash", (context) =>
      context.list.trashSelection({ permanent: false }),
    ),
    listCommand("file.deletePermanent", "Delete Permanently", (context) =>
      context.list.trashSelection({ permanent: true }),
    ),
    {
      id: "app.escape",
      title: "Escape",
      run: (context) => {
        if (context.list.exitVisual()) return true;
        if (context.checkboxes.active() && !context.list.blocked()) {
          context.checkboxes.exit();
          context.list.clearSelection();
          return true;
        }
        if (context.scope !== "list" || context.list.blocked()) return false;
        return context.list.clearSelection();
      },
    },
    { id: "file.copy", title: "Copy", run: (context) => context.list.enqueueSelected("copy") },
    { id: "file.cut", title: "Cut", run: (context) => context.list.enqueueSelected("move") },
    {
      id: "file.paste",
      title: "Paste",
      when: (context) => context.list.canPaste(),
      run: (context) => context.list.paste(),
    },
    {
      id: "search.focus",
      title: "Search This Folder",
      when: (context) => !context.focusInside("[data-archive-view]"),
      run: (context) => context.focusSearch(),
    },
    {
      id: "app.openTerminal",
      title: "Open Terminal Here",
      when: (context) => !context.list.remote() && isLocalFolder(context.currentPath()),
      run: async (context) => {
        const { openTerminalHere } = await import("$lib/terminal/open-terminal.js");
        await openTerminalHere(context.currentPath());
      },
    },
    { id: "palette.open", title: "Command Palette", run: (context) => context.togglePalette() },
    {
      id: "tab.newInOtherPane",
      title: "New Tab in Other Pane",
      run: (context) => context.panes.newTabInOtherPane(),
    },
    { id: "pane.left", title: "Focus Left Pane", run: (context) => focusPane(context, -1) },
    { id: "pane.right", title: "Focus Right Pane", run: (context) => focusPane(context, 1) },
    {
      id: "tab.goto",
      title: "Go to Tab",
      run: (context, args) => context.tabs.selectDigit(Number(args.digit)),
    },
  ];
}
