import { appVersion } from "./version";

export const site = {
  name: "Lite Explorer",
  tagline: "Less chrome. More room for your files.",
  version: `v${appVersion}`,
  stack: "Tauri 2 · SvelteKit · TypeScript",
  license: "MIT",
  platforms: ["macOS", "Linux", "Windows"],
  url: "https://explorer.litepod.sh",
  repo: "https://github.com/litepodsh/lite-explorer",
  changelog: "https://github.com/litepodsh/lite-explorer/blob/main/changelog/CHANGELOG.md",
  roadmap: "https://github.com/litepodsh/lite-explorer/blob/main/ROADMAP.md",
  description:
    "Lite Explorer is a free, open-source file manager for macOS, Windows and Linux with tabs, split panes, a command palette, content search, archive support and full keyboard control.",
  ogImage: "/og-image.png",
  ogImageAlt:
    "Lite Explorer overview screen showing disk capacity, volumes and what is using space in the home folder",
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

/**
 * Legal pages (privacy, terms) and the Google OAuth consent screen point here.
 * Keep `effectiveDate` in sync whenever the policy text changes.
 */
export const legal = {
  publisher: "Litepod Studio",
  email: "info@litepod.sh",
  effectiveDate: "September 23, 2026",
  privacyPath: "/privacy",
  termsPath: "/terms",
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
    icon: "layers",
    title: "A sidebar that gets out of the way",
    body: "Collapse it, resize it, or let it float over the content. Lite Explorer remembers the width and the floating preference between launches, so the window opens the way you left it.",
  },
  {
    tag: "Panes",
    icon: "panes",
    title: "Tabs and split panes, per column",
    body: "Open a second pane and move between folders side by side. Tabs open in the other pane with ⇧⌘T, and each pane keeps its own history.",
  },
  {
    tag: "Search",
    icon: "search",
    title: "Find files, then find inside them",
    body: "Filter the current folder instantly, or switch to content mode and search text across every file, with the matching snippet shown inline.",
  },
  {
    tag: "Archive",
    icon: "archive",
    title: "Open archives without leaving the app",
    body: "Browse the contents of an archive like any folder, then extract it in place and resolve name conflicts as they appear.",
  },
  {
    tag: "Files",
    icon: "copy",
    title: "Real file operations",
    body: "Copy, cut, paste, rename, move to Trash, permanent delete. Favorites with drag-to-reorder, multi-select, and a transfer clipboard that holds your queue.",
  },
  {
    tag: "System",
    icon: "cpu",
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

export const stats = [
  { icon: "cpu", value: "3", unit: "OS", label: "Platforms", sub: "macOS, Linux, Windows" },
  { icon: "download", value: appVersion, unit: "", label: "Latest release", sub: "Updates in place" },
  { icon: "eye", value: "5", unit: "types", label: "Built-in previews", sub: "Code, Markdown, CSV, PDF, images" },
  { icon: "scale", value: "MIT", unit: "", label: "License", sub: "Free and open source" },
];

export const featureList = [
  "Collapsible, resizable sidebar with favorites, locations and tags",
  "Tabs and split panes for side-by-side browsing",
  "Command palette (Shift+Cmd+P) for commands, folders and recent places",
  "Instant folder filter and search inside file contents",
  "Open and extract archives without leaving the app",
  "Transfer clipboard and Activity drawer for copy, move, delete and extraction jobs",
  "Built-in preview for code, Markdown, CSV, PDF and images",
  "Standard or yazi-style (Vim-like) keyboard shortcuts",
  "Remote locations: Amazon S3, Cloudflare R2, Azure Blob Storage, Google Drive, SFTP and SMB",
  "Dark and light themes, free and open source (MIT)",
];

export const preview = [
  "Code with syntax highlighting",
  "Markdown",
  "CSV tables",
  "PDF",
  "Images",
];

export const faq = [
  {
    q: "What is Lite Explorer?",
    a: "Lite Explorer is a free, open-source file manager (file explorer) for macOS, Windows and Linux. It has a macOS Finder-inspired layout with a collapsible sidebar, tabs, split panes, a command palette, content search and full keyboard control. It is built with Tauri 2, SvelteKit and TypeScript.",
  },
  {
    q: "Is Lite Explorer free?",
    a: "Yes. Lite Explorer is free and open source under the MIT license. The source code is public on GitHub, and you can download release builds at no cost.",
  },
  {
    q: "Which operating systems does Lite Explorer support?",
    a: "macOS, Windows and Linux. Download the build for your platform from the latest GitHub release.",
  },
  {
    q: "How do I install Lite Explorer on macOS?",
    a: "Download the universal DMG from the latest release, open it and drag Lite Explorer to the Applications folder. Current builds use a self-signed certificate and are not notarized by Apple, so if macOS blocks the DMG, Control-click it in Finder, choose Open, then choose Open again. Alternatively, remove the quarantine attribute in Terminal with xattr -dr com.apple.quarantine followed by the path to the DMG.",
  },
  {
    q: "Does Lite Explorer have keyboard shortcuts or Vim keys?",
    a: "Yes. Press F1 for every shortcut, Shift+Cmd+P for the command palette, Cmd+F to search the current folder and Cmd+L to go to a path. You can also switch to yazi-style mode for j/k motions, gg, chords with a which-key popup and single-key copy and paste.",
  },
  {
    q: "Can Lite Explorer connect to cloud storage or network shares?",
    a: "Yes. Besides local folders it can browse Amazon S3, Cloudflare R2, Azure Blob Storage, other S3-compatible services, Google Drive, SFTP servers and SMB shares. Connections are made directly from your computer to the provider, and credentials are kept in your operating system's secure credential store.",
  },
  {
    q: "Does Lite Explorer collect or upload my files?",
    a: "No. Lite Explorer runs on your computer and has no accounts or file-syncing server. It can send anonymous crash reports that never include file contents. See the privacy policy for the full details.",
  },
  {
    q: "How is Lite Explorer different from Finder or Windows File Explorer?",
    a: "It keeps a familiar Finder-like layout but adds a command palette, split panes, search inside file contents, an Activity drawer for long-running jobs, optional Vim-style keys and remote storage locations in a single lightweight Tauri app.",
  },
];
