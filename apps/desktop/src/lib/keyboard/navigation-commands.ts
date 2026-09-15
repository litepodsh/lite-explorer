import type { Command } from "./commands.js";
import type { AppContext, GoTarget } from "./context.js";
import { withTrailingSlash } from "./text.js";

type AppCommand = Command<AppContext>;

function listCommand(id: string, title: string, run: AppCommand["run"]): AppCommand {
  return { id, title, when: (context) => !context.list.blocked(), run };
}

function goCommand(target: GoTarget, title: string, localOnly: boolean): AppCommand {
  return {
    id: `go.${target}`,
    title,
    when: localOnly ? (context) => !context.list.remote() : undefined,
    run: (context) => context.go(target),
  };
}

const SORT_TITLES = { name: "Sort by Name", size: "Sort by Size", date: "Sort by Date", type: "Sort by Type" } as const;

/** Commands behind the extended standard bindings and Yazi mode. */
export function navigationCommands(): AppCommand[] {
  return [
    listCommand("list.halfPageDown", "Down Half a Page", (context) => context.list.moveHalfPage(1)),
    listCommand("list.halfPageUp", "Up Half a Page", (context) => context.list.moveHalfPage(-1)),
    listCommand("folder.enter", "Open Focused Item", (context) => context.list.enterOrOpenFocused()),
    listCommand("folder.parent", "Parent Folder", (context) => context.list.openParent()),
    { id: "history.back", title: "Back", run: (context) => context.tabs.history("back") },
    { id: "history.forward", title: "Forward", run: (context) => context.tabs.history("forward") },
    listCommand("selection.toggleNext", "Toggle and Move Down", (context) => context.list.toggleFocusedAndNext()),
    listCommand("selection.invert", "Invert Selection", (context) => context.list.invertSelection()),
    listCommand("selection.visual", "Visual Selection", (context, args) =>
      context.list.startVisual(args.mode === "remove" ? "remove" : "add"),
    ),
    listCommand("file.rename", "Rename", (context) => context.list.renameFocused()),
    listCommand("file.newFile", "New File", (context) => context.list.createItem("file")),
    listCommand("file.newFolder", "New Folder", (context) => context.list.createItem("folder")),
    listCommand("file.copyPath", "Copy Path", (context) => context.list.copyText("path")),
    listCommand("file.copyDirPath", "Copy Folder Path", (context) => context.list.copyText("dirPath")),
    listCommand("file.copyName", "Copy Name", (context) => context.list.copyText("name")),
    listCommand("file.copyStem", "Copy Name Without Extension", (context) => context.list.copyText("stem")),
    { id: "clipboard.clear", title: "Clear Transfer Clipboard", run: (context) => context.list.clearClipboard() },
    ...(["name", "size", "date", "type"] as const).map((column) =>
      listCommand(`sort.${column}`, SORT_TITLES[column], (context, args) =>
        context.list.sort(column, args.dir === "desc" ? "desc" : "asc"),
      ),
    ),
    { id: "view.toggleHidden", title: "Toggle Hidden Files", run: (context) => context.app.toggleHidden() },
    {
      id: "search.content",
      title: "Search File Contents",
      when: (context) => !context.list.remote(),
      run: (context) => context.searchContent(),
    },
    {
      id: "palette.goToPath",
      title: "Go to Path",
      run: (context) => context.openPalette(withTrailingSlash(context.currentPath())),
    },
    goCommand("home", "Go Home", true),
    goCommand("downloads", "Go to Downloads", true),
    goCommand("overview", "Go to Overview", false),
    goCommand("recents", "Go to Recents", false),
    goCommand("shared", "Go to Shared", false),
    { id: "tab.new", title: "New Tab", run: (context) => context.tabs.newTab() },
    { id: "tab.next", title: "Next Tab", run: (context) => context.tabs.cycle(1) },
    { id: "tab.prev", title: "Previous Tab", run: (context) => context.tabs.cycle(-1) },
    { id: "tab.moveLeft", title: "Move Tab Left", run: (context) => context.tabs.moveActive(-1) },
    { id: "tab.moveRight", title: "Move Tab Right", run: (context) => context.tabs.moveActive(1) },
    { id: "pane.toggleSecond", title: "Toggle Second Pane", run: (context) => context.panes.toggleSecond() },
    { id: "preview.toggle", title: "Toggle Preview", run: (context) => context.preview.toggle() },
    { id: "preview.scrollDown", title: "Scroll Preview Down", run: (context) => context.preview.scroll(1) },
    { id: "preview.scrollUp", title: "Scroll Preview Up", run: (context) => context.preview.scroll(-1) },
    { id: "region.next", title: "Next Region", run: (context) => context.regions.cycle(1) },
    { id: "region.prev", title: "Previous Region", run: (context) => context.regions.cycle(-1) },
    { id: "region.sidebar", title: "Focus Sidebar", run: (context) => context.regions.focus("sidebar") },
    { id: "region.list", title: "Focus File List", run: (context) => context.regions.focus("list") },
    { id: "sidebar.next", title: "Next Sidebar Item", run: (context) => context.regions.sidebarMove(1) },
    { id: "sidebar.prev", title: "Previous Sidebar Item", run: (context) => context.regions.sidebarMove(-1) },
    { id: "sidebar.open", title: "Open Sidebar Item", run: (context) => context.regions.sidebarOpen() },
    { id: "app.activity", title: "Toggle Activity", run: (context) => context.app.toggleActivity() },
    { id: "app.shortcuts", title: "Keyboard Shortcuts", run: (context) => context.app.openShortcuts() },
    { id: "app.settings", title: "Settings", run: (context) => context.app.openSettings() },
    { id: "app.toggleKeyboardMode", title: "Toggle Yazi Mode", run: (context) => context.app.toggleKeyboardMode() },
    { id: "view.list", title: "List View", run: (context) => context.app.setView("list") },
    { id: "view.grid", title: "Icon View", run: (context) => context.app.setView("grid") },
  ];
}
