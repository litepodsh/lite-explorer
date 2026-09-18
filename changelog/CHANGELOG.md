# Changelog

All notable changes to Lite Explorer. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

One file, never rewritten: every release adds a new section at the top, under `Unreleased`, and the
section stays as shipped. `apps/desktop` turns this file into the in-app release notes at build time
(`bun run gen:changelog`), and the release workflow uses it for the GitHub release body. Images go
in `assets/`, so a release can show screenshots.

## [Unreleased]

## [0.1.10] - 2026-09-18

### Removed

- Apple Intelligence name suggestions, to stop the launch crash: the bundled model dylib, the
  suggestion button in the rename field, the “Suggest Name” context menu item, its `Mod+Shift+R`
  shortcut and the “Apple Intelligence names” switch in Settings are all gone.

### Fixed

- Crash on launch on macOS: the Apple Intelligence dylib was linked weakly and never loaded from
  the app bundle, so checking availability jumped to a null pointer and killed the app seconds
  after opening.

## [0.1.9] - 2026-09-17

### Added

- Release notes in the Help menu, listing every version newest first.
- “What’s New” dialog after an update, plus a “What’s New” button in the update banner.

## [0.1.8] - 2026-09-17

### Added

- Rename suggestions on macOS.

### Fixed

- Command palette layout and command order.
- Folders no longer refresh when nothing changed.
- Release workflow validates the Sentry DSN before building.

## [0.1.7] - 2026-09-17

### Added

- Folder sizes in the file list.
- Drag left or right to go back and forward.
- Open the current folder in an external terminal.
- Developer tools in pre-1.0 builds.
- Sort by name by default.

### Fixed

- Infinite loading on Windows.
- Windows volumes missing from the sidebar.
- Long location names overflowing the sidebar.
- AI folder and file reorganization.

### Security

- Content security policy for the webview.

## [0.1.6] - 2026-09-15

### Added

- Anonymous crash reporting, off in debug builds.

## [0.1.5] - 2026-09-15

### Added

- Spanish README.

### Changed

- README rewritten around the current feature set.

## [0.1.4] - 2026-09-14

### Added

- Multiple selection, first pass.
- Search inside file contents.
- Content search and file tree inside zip archives.
- Linux title bar.

### Fixed

- Sidebar app icon and trigger alignment.
- Dragging files to other apps.
- Copy container layout.
- Menu order.

## [0.1.2] - 2026-09-14

### Changed

- Maintenance release after the signing change.

## [0.1.1] - 2026-09-14

### Added

- Apple code signing for macOS builds.

## [0.1.0] - 2026-09-13

### Added

- First release: browsing, tabs, panes, archive extraction, preview and AI-assisted file actions.
