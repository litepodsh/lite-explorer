# Roadmap

This roadmap shows what we're working on and in what order. Priorities can change.
Want something that isn't here? Open an issue as described in
[CONTRIBUTING.md](CONTRIBUTING.md#proposing-a-feature). Proposals are welcome, and the
ones many people ask for move up.

## 1. Listing speed (top priority)

A file explorer is only as good as how fast it shows a folder. Until big folders feel
instant, this comes before new features.

**Done**

- Large network folders stream in batches. Sizes and dates are read only for visible rows.
- The list is virtualized, and rows are replaced, not proxied one by one.
- Watcher updates re-read only the changed files. A download finishing in a folder of
  100,000 files no longer re-lists the whole folder.
- Search no longer collects the whole tree before matching:
  - it walks on several threads;
  - it streams results as they are found;
  - it stops when a new query replaces the old one.
- On macOS, name search shows Spotlight matches first, while the walk covers hidden and
  unindexed files.
- The preview panel and its viewers no longer load at startup. Startup JavaScript went
  from about 1.4 MB to 0.9 MB.

**Next**

- Measure cold start and first listing on a fixed set of folders (1k, 100k and 1M
  entries, local and SMB) and track the numbers per release.
- Stream large local folders the way network folders are streamed, so the first rows
  show before the folder is fully read.
- Read names and metadata in bulk on macOS (`getattrlistbulk`), instead of one `stat`
  per entry.
- Send listings in a compact binary format instead of JSON.
- Search: find unsaved files in Spotlight's index, and add an optional index for
  unindexed volumes.

## 2. macOS integration

- **Quick Look** (first version done): <kbd>⌘</kbd> <kbd>Y</kbd> opens the system preview
  panel for the selection, and the panel follows the selection.
  - Next: open from the context menu and an optional Space binding.
  - Next: zoom from the row (`sourceFrameOnScreenForPreviewItem`).
- **Signing and notarization**, so Gatekeeper stops blocking the DMG.
- Light theme that follows the system appearance.
- Native context menus and sidebar vibrancy.

## 3. File management

- **Batch rename**: rename several files at once with a live preview of the new names.
  - Rules: find/replace (with regex), add text, number sequences, change case and dates
    from metadata.
  - Runs as a single undoable job in the Activity panel.
- **Folder sync / compare**: compare two panes and copy the differences in one or both
  directions.
- Undo for move, rename and trash.

## 4. Later

- Plugin or extension points for custom previews.
- Saved searches and smart folders.

---

Have an idea? [Open an issue](../../issues). Proposals that come with a clear use case,
and a note on how the idea affects listing speed, are the easiest to review.
