# Local File Operations: Parallel Execution + Progress Toasts — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run local copy, move, permanent delete, and trash with a bounded thread pool and report per-job file/byte progress, elapsed time, ETA, and cancellation through the existing `transfer-progress` event, so a new toast surface can display it.

**Architecture:** A new Rust module `explorer/file_transfer.rs` owns a generic worker pool, tree measurement, parallel copy/delete, and batch Tauri commands. Commands register a `CancellationToken` in the existing `TransferRegistry` and emit `TransferEvent`s through `remote::transfer::emit`, exactly like S3 transfers. The frontend creates one job per action in `JobsStore`, passes its id to the command, and renders it in the existing Activity drawer and a new toast.

**Tech Stack:** Rust (std threads, `std::thread::scope`, `mpsc`), Tauri v2, `tokio_util::CancellationToken`, Svelte 5 runes, TypeScript, Bun test.

**Spec:** `docs/superpowers/specs/2026-09-18-local-file-ops-progress-design.md`

## Global Constraints

- **Do not run `git commit`.** Leave all changes uncommitted; the user explicitly requested no commits.
- Backend runs work via `tauri::async_runtime::spawn_blocking` and awaits it; progress travels through `transfer-progress` events.
- Worker pool size is always `available_parallelism().saturating_sub(2).max(1)`, capped at the item count.
- Cancellation is passed into the pool as a closure `&(dyn Fn() -> bool + Sync)`, so commands can pass `|| token.is_cancelled()` and tests can pass `|| flag.load(Relaxed)`.
- Do not follow symlinks and do not cross device boundaries (match `src-tauri/src/explorer/sizes.rs::measure_entry`).
- Commands return `Err` only for validation failures that occur **before** the job is registered. Operation errors emit a terminal `failed` event and still return `Ok`, so the frontend never double-reports.
- macOS: wrap each top-level source/target pair in `coordinated_read`/`coordinated_write`; workers run inside that closure and never call the coordinator.
- No new dependencies.
- Frontend tests run with `bun test`; Rust tests with `cargo test`, lint with `cargo clippy`.
- Use camelCase at the TS/JSON boundary (`TransferEvent` already has `#[serde(rename_all = "camelCase")]`).

---

### Task 1: Shared progress/ETA helpers

**Files:**
- Create: `apps/desktop/src/lib/transfers/progress.ts`
- Test: `apps/desktop/src/lib/transfers/progress.test.ts`
- Modify: `apps/desktop/src/lib/transfer-clipboard/queue.ts` (re-export `formatEta`)

**Interfaces:**
- Produces: `progressFraction(done, total): number | null`, `etaSeconds(done, total, elapsedSeconds): number | null`, `formatEta(seconds): string | null`.

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/transfers/progress.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import { etaSeconds, formatEta, progressFraction } from "./progress.js";

describe("progressFraction", () => {
  test("returns null when the total is unknown or zero", () => {
    expect(progressFraction(10, 0)).toBeNull();
    expect(progressFraction(0, -1)).toBeNull();
  });
  test("clamps to 0..1", () => {
    expect(progressFraction(5, 10)).toBe(0.5);
    expect(progressFraction(20, 10)).toBe(1);
    expect(progressFraction(-1, 10)).toBe(0);
  });
});

describe("etaSeconds", () => {
  test("returns null without a rate or a total", () => {
    expect(etaSeconds(0, 100, 5)).toBeNull();
    expect(etaSeconds(10, 0, 5)).toBeNull();
    expect(etaSeconds(10, 100, 0)).toBeNull();
    expect(etaSeconds(100, 100, 5)).toBeNull();
  });
  test("extrapolates remaining time from the observed rate", () => {
    expect(etaSeconds(50, 100, 10)).toBe(10);
  });
});

describe("formatEta", () => {
  test("formats seconds and minutes", () => {
    expect(formatEta(null)).toBeNull();
    expect(formatEta(0.4)).toBe("1s left");
    expect(formatEta(30)).toBe("30s left");
    expect(formatEta(90)).toBe("2m left");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test src/lib/transfers/progress.test.ts`
Expected: FAIL — cannot resolve `./progress.js`.

- [ ] **Step 3: Write the implementation**

Create `apps/desktop/src/lib/transfers/progress.ts`:

```ts
/** Done/total as a clamped 0..1 fraction, or null when the total is unknown. */
export function progressFraction(done: number, total: number): number | null {
  if (total <= 0) return null;
  return Math.min(1, Math.max(0, done / total));
}

/** Remaining seconds from the observed rate, or null when there is not enough signal. */
export function etaSeconds(done: number, total: number, elapsedSeconds: number): number | null {
  if (total <= 0 || done <= 0 || elapsedSeconds <= 0 || done >= total) return null;
  const rate = done / elapsedSeconds;
  if (rate <= 0) return null;
  return Math.max(0, (total - done) / rate);
}

export function formatEta(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds)) return null;
  if (seconds < 60) return `${Math.ceil(seconds)}s left`;
  return `${Math.ceil(seconds / 60)}m left`;
}
```

In `apps/desktop/src/lib/transfer-clipboard/queue.ts`, replace the local `formatEta` definition (lines 123-127) with:

```ts
export { formatEta } from "../transfers/progress.js";
```

- [ ] **Step 4: Run tests**

Run: `bun test src/lib/transfers/progress.test.ts src/lib/transfer-clipboard/queue.test.ts`
Expected: PASS.

---

### Task 2: `activity.start` / `activity.fail`

**Files:**
- Modify: `apps/desktop/src/lib/transfers/jobs.ts`
- Test: `apps/desktop/src/lib/transfers/jobs.test.ts`

**Interfaces:**
- Produces: `activity.start(kind, label, destination): string`, `activity.fail(id, kind, label, destination, error): void`.

- [ ] **Step 1: Write the failing test**

Append to `apps/desktop/src/lib/transfers/jobs.test.ts` (add `TransferEventPayload` to the existing import from `./jobs.js`):

```ts
import { activity } from "./jobs.js";

describe("activity.start / fail", () => {
  test("start publishes an active cancellable job and returns its id", () => {
    const published: TransferEventPayload[] = [];
    const original = activity.publish;
    activity.publish = (event) => published.push(event);
    const id = activity.start("delete", "Delete: 3 items", "");
    activity.publish = original;
    expect(published).toHaveLength(1);
    expect(published[0]).toMatchObject({
      id,
      kind: "delete",
      state: "active",
      cancellable: true,
      filesTotal: 0,
      filesDone: 0,
    });
  });

  test("fail marks the job failed with a string error", () => {
    const published: TransferEventPayload[] = [];
    const original = activity.publish;
    activity.publish = (event) => published.push(event);
    activity.fail("job-1", "copy", "Copy: a", "/tmp", new Error("nope"));
    activity.publish = original;
    expect(published[0]).toMatchObject({ id: "job-1", state: "failed", error: "nope" });
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test src/lib/transfers/jobs.test.ts`
Expected: FAIL — `activity.start is not a function`.

- [ ] **Step 3: Write the implementation**

In `apps/desktop/src/lib/transfers/jobs.ts`, replace the `activity` object (lines 137-143) with:

```ts
/** The mounted Activity panel receives file-operation updates through this callback. */
export const activity = {
  publish: (_event: TransferEventPayload) => {},
  start(kind: JobKind, label: string, destination: string): string {
    const id = crypto.randomUUID();
    this.publish({
      id,
      kind,
      label,
      destination,
      filesTotal: 0,
      filesDone: 0,
      bytesTotal: 0,
      bytesDone: 0,
      state: "active",
      cancellable: true,
      startedAt: Date.now(),
    });
    return id;
  },
  fail(
    id: string,
    kind: JobKind,
    label: string,
    destination: string,
    error: unknown,
  ): void {
    this.publish({
      id,
      kind,
      label,
      destination,
      filesTotal: 0,
      filesDone: 0,
      bytesTotal: 0,
      bytesDone: 0,
      state: "failed",
      error: error instanceof Error ? error.message : String(error),
      startedAt: Date.now(),
      finishedAt: Date.now(),
    });
  },
  track<T>(kind: JobKind, label: string, destination: string, operation: () => Promise<T>) {
    return trackJob((event) => this.publish(event), kind, label, destination, operation);
  },
};
```

- [ ] **Step 4: Run tests**

Run: `bun test src/lib/transfers/jobs.test.ts`
Expected: PASS.

---

### Task 3: Rust module, worker pool, tree measurement

**Files:**
- Create: `apps/desktop/src-tauri/src/explorer/file_transfer.rs`
- Modify: `apps/desktop/src-tauri/src/explorer/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn worker_count(items: usize) -> usize`
  - `fn run_workers<J: Send>(items: Vec<J>, is_cancelled: &(dyn Fn() -> bool + Sync), work: impl Fn(J) + Sync + Send)`
  - `pub struct TreeStats { pub files: u64, pub bytes: u64, pub complete: bool }`
  - `pub fn measure_tree(paths: &[PathBuf], is_cancelled: &(dyn Fn() -> bool + Sync)) -> TreeStats`

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src-tauri/src/explorer/file_transfer.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("lite-file-transfer-{tag}-{}", uuid::Uuid::new_v4()))
    }

    fn never() -> bool {
        false
    }

    #[test]
    fn worker_count_reserves_threads_and_caps_at_work() {
        assert_eq!(worker_count(0), 0);
        assert_eq!(worker_count(1), 1);
        assert!(worker_count(usize::MAX) >= 1);
        assert!(worker_count(2) <= 2);
    }

    #[test]
    fn measure_tree_counts_files_and_bytes_without_following_links() {
        let root = temp_dir("measure");
        fs::create_dir_all(root.join("nested/empty")).unwrap();
        fs::write(root.join("one"), [0; 3]).unwrap();
        fs::write(root.join("nested/two"), [0; 7]).unwrap();
        let stats = measure_tree(&[root.clone()], &never);
        assert_eq!((stats.files, stats.bytes, stats.complete), (2, 10, true));

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
            let with_link = measure_tree(&[root.clone()], &never);
            let link_len = fs::symlink_metadata(root.join("loop")).unwrap().len();
            assert_eq!((with_link.files, with_link.bytes), (3, 10 + link_len));
        }

        let missing = measure_tree(&[root.join("missing")], &never);
        assert_eq!((missing.files, missing.bytes, missing.complete), (0, 0, false));

        fs::remove_dir_all(root).unwrap();
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib explorer::file_transfer`
Expected: FAIL to compile — `worker_count` / `measure_tree` not found.

- [ ] **Step 3: Write the implementation**

Add to `apps/desktop/src-tauri/src/explorer/mod.rs` (keep alphabetical):

```rust
pub mod file_transfer;
```

At the top of `file_transfer.rs`, above the test module:

```rust
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

use crate::system::volumes::device_id;

const FILE_OP_WORKER_RESERVE: usize = 2;
const FILE_OP_PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
const FILE_OP_FLUSH: usize = 64;

/// Worker threads to use for a batch of `items`, leaving a couple of cores free.
pub fn worker_count(items: usize) -> usize {
    let available = std::thread::available_parallelism()
        .map_or(4, |cores| cores.get())
        .saturating_sub(FILE_OP_WORKER_RESERVE)
        .max(1);
    available.min(items)
}

/// Runs `work` over `items` on a bounded pool, stopping when `is_cancelled` turns true.
fn run_workers<J: Send>(
    items: Vec<J>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    work: impl Fn(J) + Sync + Send,
) {
    if items.is_empty() {
        return;
    }
    let (jobs_tx, jobs_rx) = mpsc::channel::<J>();
    let jobs_rx = Arc::new(Mutex::new(jobs_rx));
    thread::scope(|scope| {
        for _ in 0..worker_count(items.len()) {
            let jobs_rx = Arc::clone(&jobs_rx);
            let work = &work;
            scope.spawn(move || {
                while let Ok(item) = jobs_rx.lock().unwrap().recv() {
                    if is_cancelled() {
                        break;
                    }
                    work(item);
                }
            });
        }
        for item in items {
            if is_cancelled() || jobs_tx.send(item).is_err() {
                break;
            }
        }
        drop(jobs_tx);
    });
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct TreeStats {
    pub files: u64,
    pub bytes: u64,
    pub complete: bool,
}

fn measure_path(root: &Path, is_cancelled: &(dyn Fn() -> bool + Sync)) -> TreeStats {
    let mut stats = TreeStats { complete: true, ..Default::default() };
    let Ok(metadata) = fs::symlink_metadata(root) else {
        stats.complete = false;
        return stats;
    };
    let device = device_id(&metadata);
    let mut pending = vec![root.to_path_buf()];
    while let Some(current) = pending.pop() {
        if is_cancelled() {
            stats.complete = false;
            return stats;
        }
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(_) => {
                stats.complete = false;
                continue;
            }
        };
        if metadata.is_dir() {
            if device.is_some() && device_id(&metadata) != device {
                stats.complete = false;
                continue;
            }
            match fs::read_dir(&current) {
                Ok(read) => {
                    for child in read {
                        match child {
                            Ok(child) => pending.push(child.path()),
                            Err(_) => stats.complete = false,
                        }
                    }
                }
                Err(_) => stats.complete = false,
            }
        } else {
            stats.files += 1;
            stats.bytes = stats.bytes.saturating_add(metadata.len());
        }
    }
    stats
}

/// Counts files and logical bytes for a set of paths, walking them in parallel.
pub fn measure_tree(
    paths: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> TreeStats {
    let results = Mutex::new(TreeStats { complete: true, ..Default::default() });
    run_workers(paths.to_vec(), is_cancelled, |path| {
        let stats = measure_path(&path, is_cancelled);
        let mut total = results.lock().unwrap();
        total.files += stats.files;
        total.bytes = total.bytes.saturating_add(stats.bytes);
        total.complete &= stats.complete;
    });
    results.into_inner().unwrap()
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib explorer::file_transfer`
Expected: PASS (2 tests).

---

### Task 4: Parallel delete internals

**Files:**
- Modify: `apps/desktop/src-tauri/src/explorer/file_transfer.rs`

**Interfaces:**
- Produces:
  - `pub fn delete_tree_parallel(roots: &[PathBuf], is_cancelled: &(dyn Fn() -> bool + Sync), on_file: impl Fn() + Sync + Send) -> bool` — returns `complete`.

- [ ] **Step 1: Write the failing test**

Append inside `tests`:

```rust
#[test]
fn delete_tree_parallel_removes_files_then_directories() {
    let root = temp_dir("delete");
    fs::create_dir_all(root.join("nested/empty")).unwrap();
    fs::write(root.join("one"), [0; 3]).unwrap();
    fs::write(root.join("nested/two"), [0; 7]).unwrap();
    let seen = std::sync::atomic::AtomicU64::new(0);
    let complete = delete_tree_parallel(&[root.clone()], &never, || {
        seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    assert!(complete);
    assert_eq!(seen.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert!(!root.exists());
}

#[test]
fn delete_tree_parallel_reports_incomplete_when_cancelled() {
    let root = temp_dir("delete-cancel");
    fs::create_dir_all(&root).unwrap();
    for index in 0..8 {
        fs::write(root.join(format!("file-{index}")), [0; 1]).unwrap();
    }
    let complete = delete_tree_parallel(&[root.clone()], &|| true, || {});
    assert!(!complete);
    fs::remove_dir_all(&root).unwrap();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib explorer::file_transfer`
Expected: FAIL to compile — `delete_tree_parallel` not found.

- [ ] **Step 3: Write the implementation**

Add to `file_transfer.rs` above the test module:

```rust
/// Collects files (including symlinks) and directories under `roots`. Directories
/// are returned deepest-first so they can be removed after their contents.
fn collect_delete_tree(
    roots: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for root in roots {
        let mut pending = vec![root.clone()];
        while let Some(current) = pending.pop() {
            if is_cancelled() {
                return (files, sort_dirs(dirs));
            }
            let Ok(metadata) = fs::symlink_metadata(&current) else {
                continue;
            };
            if metadata.is_dir() {
                dirs.push(current.clone());
                if let Ok(read) = fs::read_dir(&current) {
                    for child in read.flatten() {
                        pending.push(child.path());
                    }
                }
            } else {
                files.push(current);
            }
        }
    }
    (files, sort_dirs(dirs))
}

fn sort_dirs(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut dirs = dirs;
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));
    dirs
}

/// Removes the trees in parallel, files first then directories.
/// Returns `false` when cancelled or when some entries could not be removed.
pub fn delete_tree_parallel(
    roots: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_file: impl Fn() + Sync + Send,
) -> bool {
    let (files, dirs) = collect_delete_tree(roots, is_cancelled);
    if is_cancelled() {
        return false;
    }
    let files_ok = Mutex::new(true);
    run_workers(files, is_cancelled, |path| {
        if fs::remove_file(&path).is_ok() {
            on_file();
        } else {
            *files_ok.lock().unwrap() = false;
        }
    });
    if is_cancelled() {
        return false;
    }
    let dirs_ok = Mutex::new(true);
    run_workers(dirs, is_cancelled, |path| {
        if fs::remove_dir(&path).is_err() {
            *dirs_ok.lock().unwrap() = false;
        }
    });
    files_ok.into_inner().unwrap() && dirs_ok.into_inner().unwrap() && !is_cancelled()
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib explorer::file_transfer`
Expected: PASS.

---

### Task 5: Parallel copy internals

**Files:**
- Modify: `apps/desktop/src-tauri/src/explorer/file_transfer.rs`

**Interfaces:**
- Produces:
  - `pub fn copy_tree_parallel(source: &Path, target: &Path, is_cancelled: &(dyn Fn() -> bool + Sync), on_bytes: impl Fn(u64) + Sync + Send, on_file: impl Fn() + Sync + Send) -> bool`

- [ ] **Step 1: Write the failing test**

Append inside `tests`:

```rust
#[test]
fn copy_tree_parallel_reproduces_the_source_tree() {
    let source = temp_dir("copy-src");
    let target = temp_dir("copy-dst");
    fs::create_dir_all(source.join("nested/empty")).unwrap();
    fs::write(source.join("one"), [0; 3]).unwrap();
    fs::write(source.join("nested/two"), [0; 7]).unwrap();
    let bytes = std::sync::atomic::AtomicU64::new(0);
    let files = std::sync::atomic::AtomicU64::new(0);
    let complete = copy_tree_parallel(
        &source,
        &target,
        &never,
        |count| {
            bytes.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
        },
        || {
            files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        },
    );
    assert!(complete);
    assert_eq!(bytes.load(std::sync::atomic::Ordering::Relaxed), 10);
    assert_eq!(files.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(fs::read(target.join("nested/two")).unwrap().len(), 7);
    assert!(target.join("nested/empty").is_dir());

    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(target).unwrap();
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --lib explorer::file_transfer`
Expected: FAIL to compile — `copy_tree_parallel` not found.

- [ ] **Step 3: Write the implementation**

Add to `file_transfer.rs` above the test module:

```rust
struct FileTask {
    source: PathBuf,
    target: PathBuf,
}

/// Walks `source`, creating the matching directory skeleton under `target` and
/// collecting the files to copy. Returns the tasks and whether the walk was complete.
fn collect_copy_tree(
    source: &Path,
    target: &Path,
    device: Option<u64>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> (Vec<FileTask>, bool) {
    let mut tasks = Vec::new();
    let mut complete = true;
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((from, to)) = pending.pop() {
        if is_cancelled() {
            complete = false;
            break;
        }
        let Ok(metadata) = fs::symlink_metadata(&from) else {
            complete = false;
            continue;
        };
        if metadata.is_dir() {
            if device.is_some() && device_id(&metadata) != device {
                complete = false;
                continue;
            }
            if fs::create_dir_all(&to).is_err() {
                complete = false;
                continue;
            }
            match fs::read_dir(&from) {
                Ok(read) => {
                    for child in read {
                        match child {
                            Ok(child) => pending.push((child.path(), to.join(child.file_name()))),
                            Err(_) => complete = false,
                        }
                    }
                }
                Err(_) => complete = false,
            }
        } else {
            tasks.push(FileTask { source: from, target: to });
        }
    }
    (tasks, complete)
}

/// Copies one tree in parallel. `on_bytes` reports each file's byte delta and
/// `on_file` fires once per completed file. Returns `false` when cancelled or
/// when any file failed to copy.
pub fn copy_tree_parallel(
    source: &Path,
    target: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_bytes: impl Fn(u64) + Sync + Send,
    on_file: impl Fn() + Sync + Send,
) -> bool {
    let device = fs::symlink_metadata(source).ok().and_then(|m| device_id(&m));
    let (tasks, mut complete) = collect_copy_tree(source, target, device, is_cancelled);
    if is_cancelled() {
        return false;
    }
    let copied = Mutex::new(true);
    run_workers(tasks, is_cancelled, |task| match fs::copy(&task.source, &task.target) {
        Ok(bytes) => {
            on_bytes(bytes);
            on_file();
        }
        Err(_) => *copied.lock().unwrap() = false,
    });
    complete &= copied.into_inner().unwrap();
    complete && !is_cancelled()
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib explorer::file_transfer`
Expected: PASS.

---

### Task 6: Progress emitter, target planning, batch commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/explorer/file_transfer.rs`
- Modify: `apps/desktop/src-tauri/src/remote/transfer.rs` (add `pub fn timestamp_ms`)
- Modify: `apps/desktop/src-tauri/src/lib.rs:78-165` (handler list)

**Interfaces:**
- Consumes: `remote::transfer::{self, TransferEvent, TransferRegistry, timestamp_ms}`, `explorer::entries::{coordinated_read, coordinated_write, single_entry, unique_name, DirectoryEntry}`, `explorer::recents::{insert_recent, recent_kind}`, `app::db::Database`, `explorer::file_ops::trash_local_item`.
- Produces:
  - Tauri commands `copy_items`, `move_items`, `delete_items`.
  - `fn plan_targets(paths: &[String], destination: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String>`
  - `pub fn reserve_unique_name(parent: &Path, base: &str, used: &mut Vec<String>) -> String`
  - `fn same_device(source: &Path, destination: &Path) -> bool`

- [ ] **Step 1: Write the failing tests**

Append inside `tests`:

```rust
#[test]
fn reserve_unique_name_avoids_existing_and_reserved_names() {
    let parent = temp_dir("reserve");
    fs::create_dir_all(&parent).unwrap();
    fs::write(parent.join("a.txt"), []).unwrap();
    let mut used = Vec::new();
    assert_eq!(reserve_unique_name(&parent, "a.txt", &mut used), "a_2.txt");
    assert_eq!(reserve_unique_name(&parent, "a.txt", &mut used), "a_3.txt");
    fs::remove_dir_all(parent).unwrap();
}

#[test]
fn plan_targets_rejects_missing_destination() {
    let missing = temp_dir("plan-missing");
    let error = plan_targets(&["/tmp/x".to_string()], &missing).unwrap_err();
    assert!(error.contains("not a directory"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib explorer::file_transfer`
Expected: FAIL to compile — `reserve_unique_name` / `plan_targets` not found.

- [ ] **Step 3: Expose a timestamp helper**

In `apps/desktop/src-tauri/src/remote/transfer.rs`, change the existing private `fn timestamp_ms()` (line 88) to `pub fn timestamp_ms()`.

- [ ] **Step 4: Write the progress emitter and helpers**

Add these imports at the top of `file_transfer.rs`, next to the existing ones:

```rust
use serde::Serialize;
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

use crate::app::db::Database;
use crate::explorer::entries::{
    coordinated_read, coordinated_write, single_entry, unique_name, DirectoryEntry,
};
use crate::explorer::recents::{insert_recent, recent_kind};
use crate::remote::transfer::{self, timestamp_ms, TransferEvent, TransferRegistry};
```

Add above the test module:

```rust
struct FileOpProgress {
    app: AppHandle,
    token: CancellationToken,
    id: String,
    kind: String,
    label: String,
    destination: String,
    files_total: u64,
    bytes_total: u64,
    started_at: u64,
    inner: Mutex<ProgressInner>,
}

struct ProgressInner {
    files_done: u64,
    bytes_done: u64,
    current: String,
    last_emit: std::time::Instant,
}

impl FileOpProgress {
    #[allow(clippy::too_many_arguments)]
    fn new(
        app: AppHandle,
        token: CancellationToken,
        id: String,
        kind: &str,
        label: String,
        destination: String,
        files_total: u64,
        bytes_total: u64,
    ) -> Self {
        let progress = Self {
            app,
            token,
            id,
            kind: kind.to_string(),
            label,
            destination,
            files_total,
            bytes_total,
            started_at: timestamp_ms(),
            inner: Mutex::new(ProgressInner {
                files_done: 0,
                bytes_done: 0,
                current: String::new(),
                last_emit: std::time::Instant::now(),
            }),
        };
        progress.emit();
        progress
    }

    fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn event(&self) -> TransferEvent {
        let inner = self.inner.lock().unwrap();
        TransferEvent {
            id: self.id.clone(),
            kind: self.kind.clone(),
            label: if inner.current.is_empty() {
                self.label.clone()
            } else {
                inner.current.clone()
            },
            destination: self.destination.clone(),
            files_total: self.files_total,
            files_done: inner.files_done,
            bytes_total: self.bytes_total,
            bytes_done: inner.bytes_done,
            state: "active".to_string(),
            error: None,
            started_at: self.started_at,
            finished_at: None,
            file_progress: None,
        }
    }

    fn emit(&self) {
        transfer::emit(&self.app, &self.event());
    }

    fn add_bytes(&self, bytes: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.bytes_done = inner.bytes_done.saturating_add(bytes);
        if inner.last_emit.elapsed() >= FILE_OP_PROGRESS_INTERVAL {
            inner.last_emit = std::time::Instant::now();
            drop(inner);
            self.emit();
        }
    }

    fn finish_file(&self, name: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.files_done += 1;
        inner.current = name.to_string();
        inner.last_emit = std::time::Instant::now();
        drop(inner);
        self.emit();
    }

    fn finish(&self) {
        self.state_event(&if self.cancelled() { "cancelled" } else { "done" }, None);
    }

    fn fail(&self, error: String) {
        self.state_event("failed", Some(error));
    }

    fn state_event(&self, state: &str, error: Option<String>) {
        let mut event = self.event();
        event.state = state.to_string();
        event.error = error;
        transfer::emit(&self.app, &event);
    }
}

pub fn reserve_unique_name(parent: &Path, base: &str, used: &mut Vec<String>) -> String {
    let first = unique_name(parent, base);
    if !used.contains(&first) {
        used.push(first.clone());
        return first;
    }
    let path = Path::new(base);
    let stem = path.file_stem().and_then(|name| name.to_str()).unwrap_or(base);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let mut index = 2;
    loop {
        let candidate = format!("{stem}_{index}{extension}");
        if !parent.join(&candidate).exists() && !used.contains(&candidate) {
            used.push(candidate.clone());
            return candidate;
        }
        index += 1;
    }
}

pub fn plan_targets(paths: &[String], destination: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    if !destination.is_dir() {
        return Err(format!("{} is not a directory", destination.display()));
    }
    let mut used = Vec::new();
    let mut planned = Vec::new();
    for path in paths {
        let source = PathBuf::from(path);
        let base = source
            .file_name()
            .ok_or("invalid source path")?
            .to_string_lossy()
            .into_owned();
        let name = reserve_unique_name(destination, &base, &mut used);
        planned.push((source, destination.join(name)));
    }
    Ok(planned)
}

fn same_device(source: &Path, destination: &Path) -> bool {
    let source_device = fs::symlink_metadata(source).ok().and_then(|m| device_id(&m));
    let dest_device = fs::symlink_metadata(destination).ok().and_then(|m| device_id(&m));
    matches!((source_device, dest_device), (Some(a), Some(b)) if a == b)
}
```

- [ ] **Step 5: Write the three commands**

Add above the test module:

```rust
#[tauri::command]
pub async fn delete_items(
    app: AppHandle,
    transfers: State<'_, TransferRegistry>,
    paths: Vec<String>,
    permanent: bool,
    job_id: String,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("No paths to delete".into());
    }
    let token = transfers.register(&job_id);
    let remove_id = job_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let is_cancelled = || token.is_cancelled();
        let stats = measure_tree(&roots, &is_cancelled);
        let progress = FileOpProgress::new(
            app,
            token.clone(),
            job_id,
            "delete",
            format!("Delete {} items", roots.len()),
            String::new(),
            stats.files,
            stats.bytes,
        );
        if permanent {
            let mut complete = true;
            for root in &roots {
                complete &= coordinated_write(root, || {
                    delete_tree_parallel(
                        std::slice::from_ref(root),
                        &is_cancelled,
                        || progress.finish_file(&root.to_string_lossy()),
                    )
                })
                .unwrap_or(false);
            }
            if is_cancelled() {
                progress.finish();
            } else if complete {
                progress.finish();
            } else {
                progress.fail("Some items could not be deleted".into());
            }
        } else {
            let ok = Mutex::new(true);
            run_workers(roots, &is_cancelled, |root| {
                let result = coordinated_write(&root, || {
                    crate::explorer::file_ops::trash_local_item(&root)
                });
                match result {
                    Ok(()) => progress
                        .finish_file(&root.file_name().unwrap_or_default().to_string_lossy()),
                    Err(_) => *ok.lock().unwrap() = false,
                }
            });
            if is_cancelled() {
                progress.finish();
            } else if ok.into_inner().unwrap() {
                progress.finish();
            } else {
                progress.fail("Some items could not be moved to the Trash".into());
            }
        }
    })
    .await
    .map_err(|error| error.to_string());
    transfers.remove(&remove_id);
    result
}

#[tauri::command]
pub async fn copy_items(
    app: AppHandle,
    transfers: State<'_, TransferRegistry>,
    database: State<'_, Database>,
    paths: Vec<String>,
    destination: String,
    job_id: String,
) -> Result<Vec<DirectoryEntry>, String> {
    let dest = PathBuf::from(&destination);
    let planned = plan_targets(&paths, &dest)?;
    if planned.is_empty() {
        return Err("No paths to copy".into());
    }
    let token = transfers.register(&job_id);
    let remove_id = job_id.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<DirectoryEntry>, String> {
        let roots: Vec<PathBuf> = planned.iter().map(|(source, _)| source.clone()).collect();
        let is_cancelled = || token.is_cancelled();
        let stats = measure_tree(&roots, &is_cancelled);
        let progress = FileOpProgress::new(
            app.clone(),
            token.clone(),
            job_id,
            "copy",
            format!("Copy {} items", planned.len()),
            destination.clone(),
            stats.files,
            stats.bytes,
        );
        let produced = Mutex::new(Vec::<DirectoryEntry>::new());
        let mut complete = true;
        for (source, target) in &planned {
            let result = coordinated_read(source, || {
                coordinated_write(target, || {
                    copy_tree_parallel(
                        source,
                        target,
                        &is_cancelled,
                        |bytes| progress.add_bytes(bytes),
                        || progress.finish_file(&target.to_string_lossy()),
                    );
                    single_entry(target)
                })
            });
            match result {
                Ok(entry) => produced.lock().unwrap().push(entry),
                Err(_) => complete = false,
            }
        }
        if is_cancelled() {
            progress.finish();
        } else if complete {
            progress.finish();
        } else {
            progress.fail("Some items could not be copied".into());
        }
        Ok(produced.into_inner().unwrap())
    })
    .await
    .map_err(|error| error.to_string())?;
    transfers.remove(&remove_id);
    for entry in &entries {
        let _ = insert_recent(
            &database.0,
            &entry.path,
            &entry.name,
            &recent_kind(entry.is_directory),
        )
        .await;
    }
    Ok(entries)
}

#[tauri::command]
pub async fn move_items(
    app: AppHandle,
    transfers: State<'_, TransferRegistry>,
    database: State<'_, Database>,
    paths: Vec<String>,
    destination: String,
    job_id: String,
) -> Result<Vec<DirectoryEntry>, String> {
    let dest = PathBuf::from(&destination);
    let planned = plan_targets(&paths, &dest)?;
    if planned.is_empty() {
        return Err("No paths to move".into());
    }
    let token = transfers.register(&job_id);
    let remove_id = job_id.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<DirectoryEntry>, String> {
        let roots: Vec<PathBuf> = planned.iter().map(|(source, _)| source.clone()).collect();
        let is_cancelled = || token.is_cancelled();
        let cross_device = planned
            .iter()
            .any(|(source, _)| !same_device(source, &dest));
        let stats = if cross_device {
            measure_tree(&roots, &is_cancelled)
        } else {
            TreeStats { files: roots.len() as u64, bytes: 0, complete: true }
        };
        let progress = FileOpProgress::new(
            app.clone(),
            token.clone(),
            job_id,
            "move",
            format!("Move {} items", planned.len()),
            destination.clone(),
            stats.files,
            stats.bytes,
        );
        let produced = Mutex::new(Vec::<DirectoryEntry>::new());
        let mut complete = true;
        for (source, target) in &planned {
            let result = coordinated_write(source, || {
                coordinated_write(target, || {
                    let moved = fs::rename(source, target).is_ok()
                        || (copy_tree_parallel(
                            source,
                            target,
                            &is_cancelled,
                            |bytes| progress.add_bytes(bytes),
                            || {},
                        ) && delete_tree_parallel(
                            std::slice::from_ref(source),
                            &is_cancelled,
                            || {},
                        ));
                    if moved {
                        progress.finish_file(&target.to_string_lossy());
                    }
                    single_entry(target)
                })
            });
            match result {
                Ok(entry) => produced.lock().unwrap().push(entry),
                Err(_) => complete = false,
            }
        }
        if is_cancelled() {
            progress.finish();
        } else if complete {
            progress.finish();
        } else {
            progress.fail("Some items could not be moved".into());
        }
        Ok(produced.into_inner().unwrap())
    })
    .await
    .map_err(|error| error.to_string())?;
    transfers.remove(&remove_id);
    for entry in &entries {
        let _ = insert_recent(
            &database.0,
            &entry.path,
            &entry.name,
            &recent_kind(entry.is_directory),
        )
        .await;
    }
    Ok(entries)
}
```

- [ ] **Step 6: Register the commands**

In `apps/desktop/src-tauri/src/lib.rs`, near `explorer::file_ops::copy_item,` (around line 150), add:

```rust
            explorer::file_transfer::copy_items,
            explorer::file_transfer::move_items,
            explorer::file_transfer::delete_items,
```

- [ ] **Step 7: Run tests and clippy**

Run: `cargo test --lib explorer::file_transfer && cargo clippy --lib`
Expected: PASS, no new warnings.

---

### Task 7: Frontend batch API and wiring

**Files:**
- Modify: `apps/desktop/src/lib/file-ops/files.ts`
- Modify: `apps/desktop/src/lib/file-pane/controller.svelte.ts:740-794`
- Modify: `apps/desktop/src/routes/+page.svelte:57,880-886`

**Interfaces:**
- Produces: `copyItems(paths, destination, label)`, `moveItems(paths, destination, label)`, `deleteItems(paths, label)`, `trashItems(paths, label)`.

- [ ] **Step 1: Add the batch functions**

In `apps/desktop/src/lib/file-ops/files.ts`, add:

```ts
export async function copyItems(
  paths: string[],
  destination: string,
  label: string,
): Promise<DirectoryEntry[]> {
  const id = activity.start("copy", label, destination);
  try {
    return await invoke<DirectoryEntry[]>("copy_items", { paths, destination, jobId: id });
  } catch (error) {
    activity.fail(id, "copy", label, destination, error);
    throw error;
  }
}

export async function moveItems(
  paths: string[],
  destination: string,
  label: string,
): Promise<DirectoryEntry[]> {
  const id = activity.start("move", label, destination);
  try {
    return await invoke<DirectoryEntry[]>("move_items", { paths, destination, jobId: id });
  } catch (error) {
    activity.fail(id, "move", label, destination, error);
    throw error;
  }
}

export async function deleteItems(paths: string[], label: string): Promise<void> {
  const id = activity.start("delete", label, "");
  try {
    await invoke<void>("delete_items", { paths, permanent: true, jobId: id });
  } catch (error) {
    activity.fail(id, "delete", label, "", error);
    throw error;
  }
}

export async function trashItems(paths: string[], label: string): Promise<void> {
  const id = activity.start("delete", label, "");
  try {
    await invoke<void>("delete_items", { paths, permanent: false, jobId: id });
  } catch (error) {
    activity.fail(id, "delete", label, "", error);
    throw error;
  }
}
```

- [ ] **Step 2: Wire the controller**

In `apps/desktop/src/lib/file-pane/controller.svelte.ts`, replace the `onconfirm` body (lines 774-792) with:

```ts
      onconfirm: async () => {
        const paths = targets.map((target) => target.path);
        try {
          if (bucket) {
            for (const path of paths) await deleteRemoteBucket(path);
          } else if (remote) {
            await deleteRemoteItems(paths);
          } else {
            const label = `${irreversible ? "Delete" : "Move to Trash"}: ${subject}`;
            if (irreversible) await deleteItems(paths, label);
            else await trashItems(paths, label);
          }
        } catch (error) {
          await this.refreshListing(folder);
          if (remote || bucket)
            await this.showError(`Couldn’t delete ${targetLabel(targets)}`, error);
          else this.listingError = error instanceof Error ? error.message : String(error);
          return;
        }
        this.removeEntries(paths);
        if (nextFocus && this.selection.paths.length === 0) {
          this.setSelection({ paths: [], anchor: nextFocus, focus: nextFocus });
        }
      },
```

Add `deleteItems` and `trashItems` to the existing import from `$lib/file-ops/files.js` at the top of the controller.

- [ ] **Step 3: Wire the paste flow**

In `apps/desktop/src/routes/+page.svelte` around line 881, replace the per-entry loop with batch calls:

```ts
const request = transferClipboard.beginPaste(destination);
if (request) {
  const copyPaths = request.entries.filter((entry) => entry.mode === "copy").map((entry) => entry.path);
  const movePaths = request.entries.filter((entry) => entry.mode === "move").map((entry) => entry.path);
  if (copyPaths.length) await copyItems(copyPaths, destination, `Copy: ${copyPaths.length} items`);
  if (movePaths.length) await moveItems(movePaths, destination, `Move: ${movePaths.length} items`);
}
```

Update the import at `+page.svelte:57` to include `copyItems` and `moveItems`.

- [ ] **Step 4: Typecheck**

Run: `bun run check`
Expected: no new type errors.

---

### Task 8: Toast state helpers

**Files:**
- Create: `apps/desktop/src/lib/transfers/toasts.ts`
- Test: `apps/desktop/src/lib/transfers/toasts.test.ts`

**Interfaces:**
- Produces: `TOAST_LIMIT`, `TOAST_DISMISS_MS`, `visibleToasts`, `overflowCount`, `shouldAutoDismiss`.

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/transfers/toasts.test.ts`:

```ts
import { describe, expect, test } from "bun:test";
import type { Job } from "./jobs.js";
import { overflowCount, shouldAutoDismiss, visibleToasts } from "./toasts.js";

function job(overrides: Partial<Job>): Job {
  return {
    id: "job",
    kind: "delete",
    label: "Delete: a",
    destination: "",
    filesTotal: 1,
    filesDone: 1,
    bytesTotal: 0,
    bytesDone: 0,
    state: "done",
    startedAt: 0,
    ...overrides,
  };
}

describe("visibleToasts", () => {
  test("drops dismissed jobs and caps the list", () => {
    const jobs = [job({ id: "a" }), job({ id: "b" }), job({ id: "c" })];
    expect(visibleToasts(jobs, { b: true }, 2).map((item) => item.id)).toEqual(["c", "a"]);
  });
});

describe("overflowCount", () => {
  test("counts jobs beyond the limit", () => {
    const jobs = [job({ id: "a" }), job({ id: "b" }), job({ id: "c" })];
    expect(overflowCount(jobs, {}, 2)).toBe(1);
  });
});

describe("shouldAutoDismiss", () => {
  test("dismisses finished jobs after the delay only", () => {
    expect(shouldAutoDismiss(job({ state: "done", finishedAt: 0 }), 7000)).toBe(true);
    expect(shouldAutoDismiss(job({ state: "done", finishedAt: 0 }), 1000)).toBe(false);
    expect(shouldAutoDismiss(job({ state: "failed", finishedAt: 0 }), 7000)).toBe(false);
    expect(shouldAutoDismiss(job({ state: "active" }), 7000)).toBe(false);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `bun test src/lib/transfers/toasts.test.ts`
Expected: FAIL — cannot resolve `./toasts.js`.

- [ ] **Step 3: Write the implementation**

Create `apps/desktop/src/lib/transfers/toasts.ts`:

```ts
import type { Job } from "./jobs.js";

export const TOAST_LIMIT = 4;
export const TOAST_DISMISS_MS = 6000;

/** Newest-first jobs that are not dismissed, capped at `limit`. */
export function visibleToasts(
  jobs: Job[],
  dismissed: Record<string, true>,
  limit = TOAST_LIMIT,
): Job[] {
  return jobs.filter((job) => !dismissed[job.id]).slice(-limit).reverse();
}

export function overflowCount(
  jobs: Job[],
  dismissed: Record<string, true>,
  limit = TOAST_LIMIT,
): number {
  const count = jobs.filter((job) => !dismissed[job.id]).length;
  return Math.max(0, count - limit);
}

export function shouldAutoDismiss(job: Job, now: number): boolean {
  return (
    (job.state === "done" || job.state === "cancelled") &&
    job.finishedAt !== undefined &&
    now - job.finishedAt > TOAST_DISMISS_MS
  );
}
```

- [ ] **Step 4: Run the tests**

Run: `bun test src/lib/transfers/toasts.test.ts`
Expected: PASS.

---

### Task 9: Toast component and mount

**Files:**
- Create: `apps/desktop/src/lib/transfers/transfer-toast.svelte`
- Modify: `apps/desktop/src/routes/+page.svelte` (import near line 60, mount near line 1094)

**Interfaces:**
- Produces: `<TransferToast {jobs} />`, consuming `JobsStore`, `visibleToasts`, `overflowCount`, `shouldAutoDismiss`, `progressFraction`, `etaSeconds`, `formatEta`, `formatJobDuration`.

- [ ] **Step 1: Write the component**

Create `apps/desktop/src/lib/transfers/transfer-toast.svelte`:

```svelte
<script lang="ts">
  import { formatJobDuration, type Job } from "$lib/transfers/jobs.js";
  import { etaSeconds, formatEta, progressFraction } from "$lib/transfers/progress.js";
  import { overflowCount, shouldAutoDismiss, visibleToasts } from "$lib/transfers/toasts.js";
  import type { JobsStore } from "$lib/transfers/jobs.svelte.js";

  let { jobs }: { jobs: JobsStore } = $props();

  let now = $state(Date.now());
  let dismissed = $state<Record<string, true>>({});

  $effect(() => {
    const timer = setInterval(() => {
      now = Date.now();
      for (const job of jobs.jobs) {
        if (shouldAutoDismiss(job, now)) dismissed[job.id] = true;
      }
    }, 500);
    return () => clearInterval(timer);
  });

  const shown = $derived(visibleToasts(jobs.jobs, dismissed));
  const overflow = $derived(overflowCount(jobs.jobs, dismissed));

  function fractionOf(job: Job): number {
    return (
      progressFraction(job.bytesDone, job.bytesTotal) ??
      progressFraction(job.filesDone, job.filesTotal) ??
      0
    );
  }

  function etaOf(job: Job): string | null {
    const elapsed = (now - job.startedAt) / 1000;
    const byBytes = etaSeconds(job.bytesDone, job.bytesTotal, elapsed);
    return formatEta(byBytes ?? etaSeconds(job.filesDone, job.filesTotal, elapsed));
  }
</script>

<div class="toast-stack">
  {#each shown as job (job.id)}
    {@const fraction = fractionOf(job)}
    {@const eta = etaOf(job)}
    <article class="toast" class:toast-failed={job.state === "failed"}>
      <div class="core">
        <header>
          <span class="label">{job.label}</span>
          <button class="dismiss" aria-label="Dismiss" onclick={() => (dismissed[job.id] = true)}>×</button>
        </header>
        <div class="bar" role="progressbar" aria-valuenow={Math.round(fraction * 100)}>
          <div class="fill" style:transform="scaleX({fraction})"></div>
        </div>
        <footer>
          <span class="tabular-nums">{job.filesDone} / {job.filesTotal} files</span>
          <span class="tabular-nums">{formatJobDuration(job, now)}</span>
          {#if eta}<span class="tabular-nums">{eta}</span>{/if}
          {#if job.error}<span class="error">{job.error}</span>{/if}
          {#if job.state === "active" && job.cancellable}
            <button class="cancel" onclick={() => jobs.cancel(job.id)}>Cancel</button>
          {/if}
        </footer>
      </div>
    </article>
  {/each}
  {#if overflow > 0}<p class="overflow">{overflow} more</p>{/if}
</div>

<style>
  .toast-stack {
    position: fixed;
    right: 1rem;
    bottom: 2.5rem;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 22rem;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    padding: 0.375rem;
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.1);
    animation: toast-in 500ms cubic-bezier(0.32, 0.72, 0, 1) both;
  }
  .core {
    border-radius: calc(1rem - 0.375rem);
    background: #1a1a1c;
    box-shadow: inset 0 1px 1px rgba(255, 255, 255, 0.08);
    padding: 0.75rem;
    color: #e7e5e4;
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8125rem;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    margin: 0.625rem 0 0.5rem;
    height: 0.25rem;
    border-radius: 9999px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    transform-origin: left;
    background: #0a9bff;
    transition: transform 500ms cubic-bezier(0.32, 0.72, 0, 1);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem 0.75rem;
    font-size: 0.6875rem;
    color: #9c9895;
  }
  .error {
    color: #ff6b6b;
  }
  .dismiss,
  .cancel {
    background: none;
    border: 0;
    color: #9c9895;
    cursor: pointer;
    font: inherit;
  }
  .cancel {
    color: #0a9bff;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateY(0.75rem);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
```

- [ ] **Step 2: Mount it**

In `apps/desktop/src/routes/+page.svelte`, add next to `ActivityDrawer` (line 60):

```ts
  import TransferToast from "$lib/transfers/transfer-toast.svelte";
```

Render once after `ActivityDrawer` (around line 1094):

```svelte
    <TransferToast {jobs} />
```

- [ ] **Step 3: Typecheck**

Run: `bun run check`
Expected: no new type errors.

---

### Task 10: Full verification

**Files:** none.

- [ ] **Step 1: Rust tests and lint**

Run: `cargo test --lib && cargo clippy --lib`
Workdir: `apps/desktop/src-tauri`
Expected: all tests pass, no new clippy warnings.

- [ ] **Step 2: Frontend tests**

Run: `bun test`
Workdir: `apps/desktop`
Expected: all tests pass, including `progress`, `toasts`, and `jobs`.

- [ ] **Step 3: Typecheck**

Run: `bun run check`
Workdir: `apps/desktop`
Expected: no new errors.

- [ ] **Step 4: Manual verification**

Build and run the app, then verify:
1. Multi-select several items and choose Delete → one toast, file counts climb, elapsed ticks, ETA appears once bytes flow, job finishes.
2. Copy a large folder → cancellation from the toast and Activity drawer works; partial copy remains; terminal event says cancelled.
3. Move within one volume completes instantly with item-count progress; move across volumes (external disk) shows byte progress via copy+delete fallback.
4. Trash several items → items trashed, toast reports item counts.
5. Delete a protected path → toast shows `failed` and persists.

---

## Self-Review

**Spec coverage:** Foundation (job model + toast) → Tasks 2, 8, 9. File-level pool sized cores-2 → Tasks 3-5. Pre-scan exact totals → Task 3, used in Task 6. Copy → Tasks 5-6. Delete + trash → Tasks 4, 6. Cross-volume move / same-device rename → Task 6. Cancellation via `TransferRegistry` + `cancel_transfer` → Task 6. macOS coordination → Task 6. Error contract (Err only before registration) → Task 6, Task 7. One job per action → Tasks 2, 7. Toast visual + ETA + elapsed + cancel → Tasks 1, 8, 9. Testing → every task; Task 10 runs everything. Phases 2-4 out of scope by design.

**Placeholder scan:** no TBDs; every code step is complete and self-contained.

**Type consistency:** job ids are strings end-to-end; `TransferEvent` is camelCase at the boundary; helper names `progressFraction`, `etaSeconds`, `formatEta`, `visibleToasts`, `overflowCount`, `shouldAutoDismiss` are consistent across Tasks 1, 8, 9; Rust pool signatures use the cancellation closure consistently across Tasks 3-6.

**Deferred (not gaps):** refreshing the listing on a failed terminal event after local delete is left to the toast; a future task can add a jobs-store subscription in the controller.
