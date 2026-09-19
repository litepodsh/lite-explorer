# Local File Operations: Parallel Execution + Progress Toasts

- **Date:** 2026-09-18
- **Status:** Approved, pending implementation plan
- **Phase:** Foundation (Phase 0) + Local filesystem operations (Phase 1)
- **Author:** Engineering

## 1. Summary

Local filesystem operations (copy, move, permanent delete, trash) currently run
one selected path at a time from the frontend and report no progress or
cancellation to the user. This spec describes:

1. A shared **progress + cancellation foundation** built on the existing
   `transfer-progress` event, `TransferRegistry`, and `JobsStore` model.
2. **File-level parallel execution** for local copy, move, and delete using a
   bounded worker pool sized to `available_parallelism - 2`.
3. A **toast surface** that shows per-operation progress: files done / total,
   bytes where available, live elapsed time, ETA, and a cancel action.

Remote (S3/SFTP/LAN) transfers already emit `transfer-progress` and register
cancellation tokens, so they light up the toast without backend changes. Phases
2-4 (compress/extract, upload/download, remote migration) are explicitly out of
scope here and get their own specs.

## 2. Goals

- Copy, move, permanent delete, and trash on local disk run with a bounded
  thread pool instead of a single sequential pass.
- Each user action produces **one job** (not one per selected path) with
  accurate `filesTotal`, `bytesTotal`, `filesDone`, `bytesDone`, `startedAt`,
  and `finishedAt`.
- Active local operations are cancellable through the existing
  `cancel_transfer` command.
- A toast shows progress, file counts, elapsed time, and ETA for every
  long-running job, including transfers that already emit progress.

## 3. Non-Goals

- Remote/network transfer changes (S3, SFTP, FTP, SMB, LocalSend). They keep
  their current implementation.
- Compress/extract (`archive.rs`) progress.
- Upload/download local IPC progress.
- Changing `rename_item`; rename is atomic and instant.
- Preserving file metadata (mtime, permissions, extended attributes) beyond
  what the current implementation already preserves (nothing extra).

## 4. Existing Architecture (inventory)

Backend:

- `src-tauri/src/explorer/file_ops.rs` — local `copy_item_inner`,
  `move_item_inner`, `trash_local_item`, `delete_item_path`,
  `copy_dir_recursive`. All sequential, no progress, no cancellation.
- `src-tauri/src/explorer/sizes.rs` — parallel top-level size scan with a
  bounded worker pool, mpsc job queue, throttled progress channel, and a
  cancellation registry. Reference pattern for the new pool; also contains the
  device-boundary and symlink rules.
- `src-tauri/src/explorer/entries.rs` — `coordinated_read` / `coordinated_write`
  wrap a whole-tree closure and run on the calling thread.
- `src-tauri/src/remote/transfer.rs` — `TransferEvent`, `TransferRegistry`
  (job id -> `CancellationToken`), `emit` to `transfer-progress`,
  `cancel_transfer`.
- `src-tauri/src/remote/write.rs` — `Progress` helper wrapping `TransferEvent`
  with throttled emit, `start_file`, `add_bytes`, `finish_file`, `finish`,
  `fail`. Reference pattern for emitting.

Frontend:

- `src/lib/transfers/jobs.ts` — `Job`, `TransferEventPayload`, `JobsStore`
  helpers (`upsert`, `visible`, `trackJob`, `formatJobDuration`).
- `src/lib/transfers/jobs.svelte.ts` — `JobsStore` (reactive), `cancel()` calls
  `cancel_transfer`.
- `src/lib/components/custom/activity/activity-drawer.svelte` — existing
  Activity drawer listing jobs.
- `src/lib/file-ops/files.ts` — `copyItem`/`moveItem`/`trashItem`/`deleteItem`,
  each `activity.track(...)` around a single invoke.
- `src/lib/transfer-clipboard/queue.ts` — `etaSeconds`, `formatEta`,
  `totalBytes` helpers; source for the generalized ETA helper.
- `src/lib/file-pane/controller.svelte.ts` — `deleteEntries` fans out
  `for (const path of paths) await deleteItem(path)` (permanent) or
  `trashItem(path)`.
- `src/routes/+page.svelte` — registers `activity.publish = jobs.upsert`,
  listens to `transfer-progress`, hosts the Activity button/drawer, paste flow
  at ~line 881.

## 5. Architecture Decision

Reuse the single existing spine:

- Backend emits `transfer-progress` and registers a `CancellationToken` in
  `TransferRegistry` under the job id, exactly like S3 transfers.
- Frontend creates the job in `JobsStore` under the same id before invoking, so
  incoming events update the existing job.

Alternatives considered and rejected:

- **Per-call `Channel` (like `scan_directory_sizes`).** Forks the job model;
  the toast would need two event sources. Rejected.
- **Keep per-path commands and parallelize only from the frontend.** The
  frontend already runs on the JS event loop; real parallelism must live in
  Rust. Rejected.
- **One job per selected path.** Produces toast spam and cannot show a
  selection-wide total. Rejected in favor of one job per user action.

## 6. Backend Design

### 6.1 New Tauri commands

Local-only batch commands. Non-local paths keep the existing remote/network
routing in the frontend `controller` (remote paths already batch through
`delete_remote_items`; remote move is unsupported).

```
copy_items(paths: Vec<String>, destination: String, job_id: String, app: AppHandle) -> Result<Vec<DirectoryEntry>, String>
move_items(paths: Vec<String>, destination: String, job_id: String, app: AppHandle) -> Result<Vec<DirectoryEntry>, String>
delete_items(paths: Vec<String>, permanent: bool, job_id: String, app: AppHandle) -> Result<(), String>
```

- Commands validate that every path and the destination are absolute and local.
- Commands run the work on `tauri::async_runtime::spawn_blocking` and await it,
  emitting `transfer-progress` events throughout, then return the produced
  entries (or `()`). The promise resolves when the work finishes; progress
  arrives asynchronously via events, matching S3 timing semantics.
- Register the job id in `TransferRegistry`; remove it on completion/failure.

### 6.2 Pre-scan (exact totals)

Before the operation, walk the sources in parallel to compute exact
`filesTotal` and `bytesTotal`.

- A shared walker returns `TreeStats { files: u64, bytes: u64, complete: bool }`.
- It honors the same rules as `sizes.rs::measure_entry`: use
  `symlink_metadata`, do not follow links, and stop crossing device boundaries.
- Directories count as zero bytes; files add `metadata.len()`.
- Cancellation is checked during the walk; a cancelled pre-scan aborts the job
  with state `cancelled` and no filesystem change.
- Empty `paths` returns an error (frontend never calls with an empty list).

### 6.3 Target resolution

Resolve final target names with `unique_name` **sequentially** before starting
the pool, so concurrent workers cannot pick the same name. Create the
destination directory skeleton (for copy) up front.

### 6.4 File-level worker pool

- Size: `available_parallelism().saturating_sub(2).max(1)`, capped at the
  number of work items. Same reserve as the size scan (`DIRECTORY_SIZE_WORKER_RESERVE = 2`).
- Implemented with `std::thread::scope` and an mpsc job queue guarded by an
  `Arc<Mutex<Receiver>>`, matching `sizes.rs` and `search.rs`.
- Progress emission throttled at 120 ms (`PROGRESS_INTERVAL` / existing
  interval constant). `bytes_done` accumulates across workers; `files_done`
  increments after each completed file.

**Copy:** workers pull `(src, dst, size)`, call `fs::copy`, add bytes, emit.
Symlinks are handled exactly as today (not followed).

**Move:**
- Determine cross-volume up front: compare `device_id` of each source with the
  destination's device.
- Same device: `fs::rename` per top-level item; totals are item counts; no
  pool needed (instant). Emit start and done.
- Cross-device: fall back to the parallel copy path, then delete the source
  tree with the parallel delete path, under the same job.

**Delete (permanent):** the pre-scan collects all files and directories.
Workers `remove_file` files in parallel, then directories are removed
deepest-first. Symlinks are removed as links (`remove_file`), never followed.

**Trash:** the `trash` crate deletes a whole top-level item atomically, so
intra-item progress is impossible. Parallelize across selected top-level items
only, bounded by the pool size. Report `files_done`/`files_total` in items and
no byte bar. `TrashContext::NsFileManager` stays as-is on macOS.

### 6.5 macOS coordination

`coordinated_read` / `coordinated_write` wrap the entire operation closure on
the calling thread. Worker threads are spawned inside that closure and never
call the coordinator, so this remains correct. Keep the existing outer
coordination; only the closure body becomes parallel.

### 6.6 Cancellation semantics

- Every worker checks the `CancellationToken`.
- A cancelled copy/delete leaves partial results on disk. The final event is
  `cancelled` with `files_done` reflecting what completed.
- The toast/status message must read `Cancelled · N of M removed` (delete) or
  `Cancelled · N of M copied` (copy/move) so partial state is explicit.
- The `recents` DB insert runs only for entries produced by a successful
  top-level operation and is not cancellable.

### 6.7 Error handling

- Per-path and per-file errors are collected; a single unreadable file does not
  abort the job.
- `complete = false` is surfaced similarly to the size scan.
- If any part fails, the final event is `failed` with a message like
  `N of M items failed: <first error>`.
- If the destination is not a directory, the command returns an immediate
  error before registering the job.

## 7. Frontend Design

### 7.1 API

`src/lib/file-ops/files.ts` gains batch functions that create one job under a
caller-supplied or generated id and invoke the corresponding command:

```
copyItems(paths, destination, label) -> Promise<DirectoryEntry[]>
moveItems(paths, destination, label) -> Promise<DirectoryEntry[]>
deleteItems(paths, label)            -> Promise<void>   // permanent
trashItems(paths, label)             -> Promise<void>
```

Single-item helpers remain for callers outside the pane (e.g. remote locations)
and delegate to the batch functions with a one-element array for local paths.

### 7.2 JobsStore

Add a `start(kind, label, destination, id)` method that publishes an `active`
job with zero totals before the command is invoked, so backend events upsert
into an existing job. `activity.track` remains for operations that still do not
emit progress.

A backend terminal event is authoritative for `state`, `error`, and totals. If
the command rejects before any terminal event was emitted (validation failure
or panic), the frontend marks the job `failed` from the thrown error; if a
terminal event was already received, the rejection is ignored to avoid
overwriting it.

### 7.3 Controller

`deleteEntries` (`controller.svelte.ts:740`):

- Remote paths: existing `deleteRemoteItems` (already batched + progress).
- Remote bucket: existing per-bucket call.
- Local permanent: one `deleteItems(paths, label)` call.
- Local trash: one `trashItems(paths, label)` call.

The `for (const path of paths) await …` fan-out is removed. Paste flow in
`+page.svelte` (~line 881) uses `moveItems` / `copyItems` per queued batch.

## 8. Toast UI

New `src/lib/transfers/toasts.svelte.ts` (state) and
`src/lib/transfers/transfer-toast.svelte` (view), mounted once in
`+page.svelte`.

- Source of truth: `JobsStore`. One toast per job in a running/finished state.
- Contents: kind icon, label, destination, progress bar, `filesDone / filesTotal`
  files, live elapsed time, ETA, cancel button when `cancellable`.
- Progress fraction: `bytesDone / bytesTotal` when `bytesTotal > 0`, else
  `filesDone / filesTotal`.
- ETA: generalized from `queue.ts` (`etaSeconds` / `formatEta`) into a shared
  helper; returns null when rate is unknown.

Visual language (drawn from the high-end visual skill, adapted to this app's
existing dark palette and Lucide icons rather than the skill's landing-page
rules):

- Double-bezel: outer glass tray `rounded-2xl bg-white/5 ring-1 ring-white/10 p-1.5`,
  inner core `rounded-[calc(1rem-0.375rem)] bg-[#1a1a1c]` with an inset top
  highlight.
- Progress bar animates `transform: scaleX()` only, with
  `transition: transform 500ms cubic-bezier(0.32,0.72,0,1)`.
- Enter/exit animate `translate-y` and `opacity` only; stacked toasts stagger.
- Accent `#0a9bff`, secondary text `#9c9895`.
- Position bottom-right (clear of the status bar and Activity drawer).
- Max ~4 visible; overflow collapses to `N more`.
- Auto-dismiss `done`/`cancelled` after a short delay; keep `failed` until
  dismissed.
- Live elapsed requires a single shared ticker while any toast is visible, not
  a timer per toast.

## 9. Testing

Rust (`cargo test`):

- Pre-scan returns correct file and byte totals for nested trees, ignores
  device crossings, does not follow symlinks.
- Parallel copy reproduces the source tree and the byte total.
- Parallel delete removes the tree; cancellation mid-run leaves a partial tree
  and reports `files_done`.
- Move same-device relocates and records `recents`; cross-device fallback is
  covered where the environment allows, otherwise unit-tested at the branch
  level.
- Existing `file_ops.rs` tests keep passing.

Frontend (existing test runner):

- Job fraction, ETA, and formatting helpers.
- Toast store transitions: enqueue, update, auto-dismiss on done, persist on
  error, stack cap.

Verification commands: `cargo test`, `cargo clippy`, frontend test runner
(match the repository's configured script) in `apps/desktop`.

Manual: copy/delete/move a large folder and a multi-selection, cancel
mid-flight, verify elapsed and ETA update, verify partial warnings.

## 10. Decomposition / Rollout

This spec covers Phase 0 (foundation) and Phase 1 (local ops). Later phases are
separate specs:

- Phase 2: compress/extract progress.
- Phase 3: upload/download IPC progress.
- Phase 4: remote/network migration to the toast (no backend change expected;
  they already emit `transfer-progress` and register tokens).

## 11. Decisions Taken (vetoable during review)

- Delete is cancellable, accepting partial-tree state with an explicit warning
  message.
- Trash parallelizes across selected items only, with no intra-item byte bar.
- A shared ETA helper is extracted and used by both the clipboard queue and the
  toast.

## 12. Open Questions

None. All blocking questions were resolved during brainstorming.
