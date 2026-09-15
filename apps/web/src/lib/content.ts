import { appVersion } from "./version";

export const site = {
  name: "Lite Explorer",
  tagline: "Less chrome. More room for your files.",
  version: `v${appVersion}`,
  stack: "Tauri 2 · SvelteKit · TypeScript",
  license: "MIT",
  platforms: ["macOS", "Linux", "Windows"],
  repo: "https://github.com/litepodsh/lite-explorer",
  download: "https://github.com/litepodsh/lite-explorer/releases/latest",
  releases: "https://github.com/litepodsh/lite-explorer/releases",
  licenseUrl: "https://github.com/litepodsh/lite-explorer/blob/main/LICENSE",
  host: {
    name: "Litepod",
    domain: "litepod.sh",
    url: "https://litepod.sh",
    logo: "/litepod.svg",
    tagline: "Hosted on Litepod",
  },
};

export const hostLink = `${site.host.url}/?utm_source=lite-explorer&utm_medium=referral&utm_campaign=hosted-on-litepod`;

export const marquee = [
  "Collapsible sidebar",
  "Tabs & split panes",
  "Command palette",
  "Search inside files",
  "Yazi keyboard mode",
  "Archive extraction",
  "Favorites",
  "Multiple selection",
];

export const highlights = [
  {
    tag: "Workspace",
    title: "A sidebar that gets out of the way",
    body: "Collapse it, resize it, or let it float over the content. Lite Explorer remembers the width and the floating preference between launches, so the window opens the way you left it.",
  },
  {
    tag: "Panes",
    title: "Tabs and split panes, per column",
    body: "Open a second pane and move between folders side by side. Tabs open in the other pane with ⇧⌘T, and each pane keeps its own history.",
  },
  {
    tag: "Search",
    title: "Find files, then find inside them",
    body: "Filter the current folder instantly, or switch to content mode and search text across every file, with the matching snippet shown inline.",
  },
  {
    tag: "Archive",
    title: "Open archives without leaving the app",
    body: "Browse the contents of an archive like any folder, then extract it in place and resolve name conflicts as they appear.",
  },
  {
    tag: "Files",
    title: "Real file operations",
    body: "Copy, cut, paste, rename, move to Trash, permanent delete. Favorites with drag-to-reorder, multi-select, and a transfer clipboard that holds your queue.",
  },
  {
    tag: "System",
    title: "Native where it counts",
    body: "A Tauri 2 shell over a SvelteKit interface. Small binary, native menus, a real title bar, and updates delivered in place.",
  },
];

export const keys = [
  { combo: ["F1"], label: "Every shortcut" },
  { combo: ["⇧", "⌘", "P"], label: "Command palette" },
  { combo: ["⌘", "F"], label: "Search this folder" },
  { combo: ["⌘", "L"], label: "Go to path" },
  { combo: ["F6"], label: "Cycle sidebar, list, preview" },
  { combo: ["⌘", ","], label: "Settings" },
];
