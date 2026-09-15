<div align="center">

<img src="assets/app-icon.png" alt="Lite Explorer icon" width="128" height="128" />

# Lite Explorer

### A calm, native-feeling file explorer for your desktop.

Built with Tauri, SvelteKit, and TypeScript.

**English** · [Español](README.es.md)

![Lite Explorer overview](assets/preview-overview.png)

</div>

## Less chrome. More room for your files.

Lite Explorer is an experimental desktop file browser with a familiar
macOS-inspired layout: quick navigation, a focused toolbar, and a roomy content
area.

## Highlights

### Command palette

Press <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>P</kbd> and type. One search box finds
commands, folders by path, quick-access places, favorites, locations, and
recent folders, so you rarely need to reach for the mouse.

### Transfer clipboard and Activity drawer

Copy (<kbd>⌘</kbd> <kbd>C</kbd>) or cut (<kbd>⌘</kbd> <kbd>X</kbd>) files into
the transfer clipboard, then paste (<kbd>⌘</kbd> <kbd>V</kbd>) them wherever
you go next. Every long-running job lands in the Activity drawer: copies,
moves, deletes, archive extraction and compression, uploads, and downloads.
Filter jobs by **All**, **Active**, **Done**, or **Failed**, and follow their
progress without blocking the window.

### Overview

See your disk at a glance: total capacity, free space, mounted volumes, what
is using your home folder, and your Trash.

### And more

- Tabs and a second pane for side-by-side browsing
- Built-in preview for code, Markdown, CSV, PDF, and images
- Standard or yazi-style keyboard shortcuts
- Collapsible, resizable sidebar with favorites, locations, and tags
- Dark, keyboard-friendly interface

<div align="center">

![Lite Explorer recents](assets/preview-recents.png)

</div>

## Install on macOS

Download the universal DMG from the latest release, open it, and drag Lite Explorer to
the Applications folder.

The current builds use a self-signed certificate and are not notarized by Apple. If macOS
blocks the DMG, Control-click it in Finder, choose **Open**, then choose **Open** again.
Alternatively, remove its quarantine attribute in Terminal:

```bash
xattr -dr com.apple.quarantine "/path/to/liteexplorer_0.1.5_universal.dmg"
```

## Development

```bash
cd apps/desktop

# List available just recipes
just

# Install dependencies
just install

# Run development server
just dev
```

## Stack

Tauri 2 · SvelteKit · TypeScript

## License

[MIT](LICENSE) © 2026 Litepod Studio
