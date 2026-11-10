# `drag` (vendored)

Copy of the [`drag`](https://github.com/crabnebula-dev/drag-rs) crate 2.1.1, the engine
behind `tauri-plugin-drag`. Kept here because 2.1.1 is the latest release and its Windows
implementation aborts the whole app when a dragged file lives on a network share.

## Why

`drag`'s Windows `start_drag` canonicalized every path and then built a shell data object
from it. On Windows, `dunce::canonicalize` returns a verbatim path (`\\?\UNC\server\share\…`)
for UNC shares, mapped network drives and paths over `MAX_PATH`, and `ILCreateFromPathW`
rejects verbatim paths. The resulting `NULL` PIDL reached an `Option::unwrap()`, which
panicked on the main thread, killed the process and left nothing in Sentry: the panic hook
queues the event asynchronously, and the process dies before the transport flushes it.

Symptom: dragging a file from an SMB/network share to another app (Teams, Outlook, Explorer)
closed Lite Explorer instantly.

## Patch

- Absolute paths are passed to the shell untouched; only relative paths are canonicalized,
  and a failure falls back to the original path. UNC shares, mapped drives and long paths
  therefore reach `ILCreateFromPathW` in the form it accepts.
- `get_shell_item_array` returns `None` when a PIDL is `NULL` (and frees the PIDLs it made)
  instead of building an array the shell can't use.
- `get_file_data_object` propagates that failure instead of unwrapping it, and
  `start_drag` falls back to a plain `CF_HDROP` data object so the files can still be
  dropped in other apps.
- `SHCreateDataObject` and `GlobalAlloc` errors are returned instead of unwrapped, so a
  failed drag surfaces as a JavaScript promise rejection.

## Removing

Upstream is tracking the same bug in
[crabnebula-dev/drag-rs#72](https://github.com/crabnebula-dev/drag-rs/issues/72), with fixes
in PRs [#79](https://github.com/crabnebula-dev/drag-rs/pull/79) and
[#101](https://github.com/crabnebula-dev/drag-rs/pull/101). Once a release carries them,
delete this directory and the `[patch.crates-io]` entry in `src-tauri/Cargo.toml`.
