# Tauri + SvelteKit + TypeScript

This template should help get you started developing with Tauri, SvelteKit and TypeScript in Vite.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).

## Keyboard

Lite Explorer can be used without a mouse.

- `F1` or `Cmd/Ctrl+/` lists every shortcut. Search by key (`gg`, `ctrl+d`, `⇧⌘P`) or by description, or press `Cmd/Ctrl+K` in the list to record a key.
- `F6` and `Shift+F6` move between the sidebar, the file list and the preview. `Cmd/Ctrl+Shift+E` jumps to the sidebar.
- `Cmd/Ctrl+,` opens Settings. Under Keyboard, **Yazi mode** adds single-key shortcuts inspired by [yazi](https://github.com/sxyazi/yazi): `j`/`k` to move, `h`/`l` for folders, `g g`, `y`/`x`/`p`, `v` for visual selection, and chords with a which-key popup.

Yazi mode is off by default. It is not recommended with a screen reader, which also uses single-letter keys.

The keymap lives in `src/lib/keyboard/keymap.ts`; menu accelerators in `src/lib/keyboard/menu-accelerators.json` are shared with Rust.

## tldraw previews

`.tldraw` desktop archives and `.tldr` JSON exports open in an interactive,
read-only preview with pan, zoom, Fit, and page selection. The React-based SDK
loads only for drawings, inside a Svelte wrapper. Files are never saved or modified.
Standard tldraw shapes and embedded images/videos are supported; document scripts,
live website embeds, and externally hosted media are not loaded. Documents are
limited to 32 MB (including expanded archive contents) and 10,000 records.

Set `VITE_TLDRAW_LICENSE_KEY` in `.env.local` before a production build. Development
works without a key; production requires a valid [tldraw license](https://tldraw.dev/sdk-features/license-key)
covering the desktop app's origin. Fonts and SDK assets are bundled locally.
