<div align="center">

# Lite Explorer

### A calm, native-feeling file explorer for your desktop.

Built with Tauri, SvelteKit, and TypeScript.

![Lite Explorer preview](docs/lite-explorer-preview.png)

</div>

## Less chrome. More room for your files.

Lite Explorer is an experimental desktop file-browser interface with a familiar
macOS-inspired layout: quick navigation, a focused toolbar, and a roomy content
area.

- Collapsible, resizable sidebar
- Floating-sidebar preference that persists locally
- Dark, keyboard-friendly interface

## Install on macOS

Download the universal DMG from the latest release, open it, and drag Lite Explorer to
the Applications folder.

The current builds use a self-signed certificate and are not notarized by Apple. If macOS
blocks the DMG, Control-click it in Finder, choose **Open**, then choose **Open** again.
Alternatively, remove its quarantine attribute in Terminal:

```bash
xattr -dr com.apple.quarantine "/path/to/liteexplorer_0.1.1_universal.dmg"
```

## Development

```bash
cd apps/desktop

# See just recipes availables
just 

# Install dependencies
just install

# Run development server
just dev

```

## Stack

Tauri 2 · SvelteKit · TypeScript
