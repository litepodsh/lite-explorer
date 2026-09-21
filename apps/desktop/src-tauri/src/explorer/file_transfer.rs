use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::Duration,
};

use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;

use crate::app::db::Database;
use crate::explorer::entries::{
    coordinated_read, coordinated_write, single_entry, unique_name, DirectoryEntry,
};
use crate::explorer::local_path::{child_path, validate_directory, validate_existing, ExpectedKind};
use crate::explorer::recents::{insert_recent, recent_kind};
use crate::remote::transfer::{self, timestamp_ms, TransferEvent, TransferRegistry};
use crate::system::volumes::device_id;

const FILE_OP_WORKER_RESERVE: usize = 2;
const FILE_OP_PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
/// Files bundled per channel message while streaming a delete. Amortizes the
/// per-message lock so deleting many small files is not dominated by channel
/// overhead.
const DELETE_BATCH_SIZE: usize = 256;
/// Bounded queue depth (in batches) between the scanner and the remove workers,
/// which caps memory without letting the scanner run unboundedly ahead.
const DELETE_QUEUE_BATCHES: usize = 64;

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

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct TreeStats {
    pub files: u64,
    pub bytes: u64,
    pub complete: bool,
}

#[cfg(test)]
fn measure_path(root: &Path, is_cancelled: &(dyn Fn() -> bool + Sync)) -> TreeStats {
    let mut stats = TreeStats {
        complete: true,
        ..Default::default()
    };
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
#[cfg(test)]
pub fn measure_tree(paths: &[PathBuf], is_cancelled: &(dyn Fn() -> bool + Sync)) -> TreeStats {
    let results = Mutex::new(TreeStats {
        complete: true,
        ..Default::default()
    });
    run_workers(paths.to_vec(), is_cancelled, |path| {
        let stats = measure_path(&path, is_cancelled);
        let mut total = results.lock().unwrap();
        total.files += stats.files;
        total.bytes = total.bytes.saturating_add(stats.bytes);
        total.complete &= stats.complete;
    });
    results.into_inner().unwrap()
}

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
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
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
    delete_planned(files, dirs, is_cancelled, on_file)
}

/// Deletes an already-collected plan. Splitting collection from deletion lets a
/// caller derive exact file totals without walking the tree twice.
pub fn delete_planned(
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_file: impl Fn() + Sync + Send,
) -> bool {
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

/// Depth-first scanner feeding a streaming delete. Files are batched onto the
/// channel as they are found; directories are recorded post-order so they end
/// up deepest-first for removal once every file has been dispatched.
struct DeleteScanner<'a> {
    tx: &'a mpsc::SyncSender<Vec<PathBuf>>,
    dirs: &'a Mutex<Vec<PathBuf>>,
    is_cancelled: &'a (dyn Fn() -> bool + Sync),
    on_discovered: &'a dyn Fn(u64),
    batch: Vec<PathBuf>,
    discovered: u64,
}

impl DeleteScanner<'_> {
    fn walk(&mut self, path: &Path) -> bool {
        if (self.is_cancelled)() {
            return false;
        }
        let Ok(metadata) = fs::symlink_metadata(path) else {
            return true;
        };
        if metadata.is_dir() {
            if let Ok(read) = fs::read_dir(path) {
                for entry in read.flatten() {
                    if !self.walk(&entry.path()) {
                        return false;
                    }
                }
            }
            self.dirs.lock().unwrap().push(path.to_path_buf());
            return true;
        }
        self.batch.push(path.to_path_buf());
        self.discovered += 1;
        if self.batch.len() >= DELETE_BATCH_SIZE {
            return self.flush();
        }
        true
    }

    fn flush(&mut self) -> bool {
        if self.batch.is_empty() {
            return true;
        }
        let batch = std::mem::take(&mut self.batch);
        if self.tx.send(batch).is_err() {
            return false;
        }
        (self.on_discovered)(self.discovered);
        true
    }
}

/// Deletes trees in a single traversal. A scanner walks depth-first and streams
/// batches of files to a bounded queue while parallel workers unlink them, so
/// removal starts on the first directory read instead of after a full pre-scan.
/// Batching keeps the per-message channel/lock cost marginal when deleting many
/// small files. `on_discovered` reports the running count of files found;
/// `on_files` reports how many files a worker removed.
pub fn delete_tree_streaming(
    roots: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_discovered: impl Fn(u64),
    on_files: impl Fn(u64) + Sync + Send,
) -> bool {
    if is_cancelled() {
        return false;
    }
    let (tx, rx) = mpsc::sync_channel::<Vec<PathBuf>>(DELETE_QUEUE_BATCHES);
    let rx = Arc::new(Mutex::new(rx));
    let dirs = Mutex::new(Vec::new());
    let files_ok = AtomicBool::new(true);

    thread::scope(|scope| {
        for _ in 0..worker_count(usize::MAX) {
            let rx = Arc::clone(&rx);
            let files_ok = &files_ok;
            let on_files = &on_files;
            scope.spawn(move || loop {
                let batch = {
                    let guard = rx.lock().unwrap();
                    match guard.recv() {
                        Ok(batch) => batch,
                        Err(_) => break,
                    }
                };
                let mut removed = 0u64;
                for path in batch {
                    if is_cancelled() {
                        break;
                    }
                    if fs::remove_file(&path).is_ok() {
                        removed += 1;
                    } else {
                        files_ok.store(false, Ordering::Relaxed);
                    }
                }
                if removed > 0 {
                    on_files(removed);
                }
            });
        }

        {
            let mut scanner = DeleteScanner {
                tx: &tx,
                dirs: &dirs,
                is_cancelled,
                on_discovered: &on_discovered,
                batch: Vec::with_capacity(DELETE_BATCH_SIZE),
                discovered: 0,
            };
            for root in roots {
                if !scanner.walk(root) {
                    break;
                }
            }
            let _ = scanner.flush();
        }
        drop(tx);
    });

    if is_cancelled() || !files_ok.load(Ordering::Relaxed) {
        return false;
    }
    let dirs = sort_dirs(dirs.into_inner().unwrap());
    let dirs_ok = AtomicBool::new(true);
    run_workers(dirs, is_cancelled, |path| {
        if fs::remove_dir(&path).is_err() {
            dirs_ok.store(false, Ordering::Relaxed);
        }
    });
    dirs_ok.load(Ordering::Relaxed) && !is_cancelled()
}

struct FileTask {
    source: PathBuf,
    target: PathBuf,
    size: u64,
}

/// Walks `source`, creating the matching directory skeleton under `target` and
/// collecting the files to copy. Returns the tasks and whether the walk was
/// complete. This is the only traversal: totals are derived from the tasks, so
/// callers do not need a separate measuring pass.
fn plan_copy_tree(
    source: &Path,
    target: &Path,
    device: Option<u64>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
) -> (Vec<FileTask>, bool) {
    let tasks = Mutex::new(Vec::<FileTask>::new());
    let complete = AtomicBool::new(true);
    let mut frontier = vec![(source.to_path_buf(), target.to_path_buf())];
    // Scan one directory level at a time, in parallel. Creating each child
    // directory before scanning it means the skeleton is ready before files copy.
    while !frontier.is_empty() {
        if is_cancelled() {
            complete.store(false, Ordering::Relaxed);
            break;
        }
        let level = std::mem::take(&mut frontier);
        let next = Mutex::new(Vec::<(PathBuf, PathBuf)>::new());
        run_workers(level, is_cancelled, |(from, to)| {
            let Ok(metadata) = fs::symlink_metadata(&from) else {
                complete.store(false, Ordering::Relaxed);
                return;
            };
            if metadata.file_type().is_symlink() {
                complete.store(false, Ordering::Relaxed);
                return;
            }
            if !metadata.is_dir() {
                tasks.lock().unwrap().push(FileTask {
                    source: from,
                    target: to,
                    size: metadata.len(),
                });
                return;
            }
            if device.is_some() && device_id(&metadata) != device {
                complete.store(false, Ordering::Relaxed);
                return;
            }
            if fs::create_dir_all(&to).is_err() {
                complete.store(false, Ordering::Relaxed);
                return;
            }
            let Ok(read) = fs::read_dir(&from) else {
                complete.store(false, Ordering::Relaxed);
                return;
            };
            for child in read {
                match child {
                    Ok(child) => {
                        let path = child.path();
                        let target = to.join(child.file_name());
                        match fs::symlink_metadata(&path) {
                            Ok(metadata) if metadata.file_type().is_symlink() => {
                                complete.store(false, Ordering::Relaxed)
                            }
                            Ok(metadata) if metadata.is_dir() => {
                                next.lock().unwrap().push((path, target))
                            }
                            Ok(metadata) => tasks.lock().unwrap().push(FileTask {
                                source: path,
                                target,
                                size: metadata.len(),
                            }),
                            Err(_) => complete.store(false, Ordering::Relaxed),
                        }
                    }
                    Err(_) => complete.store(false, Ordering::Relaxed),
                }
            }
        });
        frontier = next.into_inner().unwrap();
    }
    let tasks = tasks.into_inner().unwrap();
    (tasks, complete.into_inner() && !is_cancelled())
}

/// Copies one file. On macOS this prefers an APFS copy-on-write clone, which is
/// effectively instant for same-volume copies; anything else falls back to a
/// normal byte copy. `clonefile` refuses to overwrite, and the target is always
/// new here.
fn copy_one(source: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        if let (Ok(source_c), Ok(target_c)) = (
            CString::new(source.as_os_str().as_bytes()),
            CString::new(target.as_os_str().as_bytes()),
        ) {
            // SAFETY: both pointers are valid NUL-terminated paths for the call.
            if unsafe { libc::clonefile(source_c.as_ptr(), target_c.as_ptr(), 0) } == 0 {
                return Ok(());
            }
        }
    }
    fs::copy(source, target).map(|_| ())
}

/// Copies already-planned tasks in parallel, reporting bytes and completed files.
fn copy_file_tasks(
    tasks: Vec<FileTask>,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_bytes: impl Fn(u64) + Sync + Send,
    on_file: impl Fn() + Sync + Send,
) -> bool {
    let copied = Mutex::new(true);
    run_workers(tasks, is_cancelled, |task| {
        if copy_one(&task.source, &task.target).is_ok() {
            on_bytes(task.size);
            on_file();
        } else {
            *copied.lock().unwrap() = false;
        }
    });
    copied.into_inner().unwrap()
}

/// Copies one tree in parallel. `on_bytes` reports each file's byte delta and
/// `on_file` fires once per completed file. Returns `false` when cancelled or
/// when any file failed to copy.
#[cfg(test)]
fn copy_tree_parallel(
    source: &Path,
    target: &Path,
    is_cancelled: &(dyn Fn() -> bool + Sync),
    on_bytes: impl Fn(u64) + Sync + Send,
    on_file: impl Fn() + Sync + Send,
) -> bool {
    let device = fs::symlink_metadata(source)
        .ok()
        .and_then(|m| device_id(&m));
    let (tasks, mut complete) = plan_copy_tree(source, target, device, is_cancelled);
    if is_cancelled() {
        return false;
    }
    complete &= copy_file_tasks(tasks, is_cancelled, on_bytes, on_file);
    complete && !is_cancelled()
}

struct FileOpProgress {
    app: AppHandle,
    token: CancellationToken,
    id: String,
    kind: String,
    label: String,
    destination: String,
    files_total: AtomicU64,
    bytes_total: u64,
    started_at: u64,
    inner: Mutex<ProgressInner>,
}

struct ProgressInner {
    files_done: u64,
    bytes_done: u64,
    current: String,
    item: Option<String>,
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
            files_total: AtomicU64::new(files_total),
            bytes_total,
            started_at: timestamp_ms(),
            inner: Mutex::new(ProgressInner {
                files_done: 0,
                bytes_done: 0,
                current: String::new(),
                item: None,
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
            files_total: self.files_total.load(Ordering::Relaxed),
            files_done: inner.files_done,
            bytes_total: self.bytes_total,
            bytes_done: inner.bytes_done,
            state: "active".to_string(),
            error: None,
            started_at: self.started_at,
            finished_at: None,
            file_progress: None,
            item: inner.item.clone(),
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
        // Emitting once per file floods the IPC bridge on trees with many small
        // files, so coalesce updates to the same interval as byte progress.
        if inner.last_emit.elapsed() >= FILE_OP_PROGRESS_INTERVAL {
            inner.last_emit = std::time::Instant::now();
            drop(inner);
            self.emit();
        }
    }

    /// Records several completed files at once, so batch deletions do not pay a
    /// lock and emit per file.
    fn add_files(&self, count: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.files_done = inner.files_done.saturating_add(count);
        if inner.last_emit.elapsed() >= FILE_OP_PROGRESS_INTERVAL {
            inner.last_emit = std::time::Instant::now();
            drop(inner);
            self.emit();
        }
    }

    /// Grows the running total as a stream discovers files. Never decreases, and
    /// emits at most once per progress interval so a fast scan cannot flood IPC.
    fn set_total(&self, files: u64) {
        if self.files_total.fetch_max(files, Ordering::Relaxed) >= files {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        if inner.last_emit.elapsed() >= FILE_OP_PROGRESS_INTERVAL {
            inner.last_emit = std::time::Instant::now();
            drop(inner);
            self.emit();
        }
    }

    /// Emits immediately once a whole queued item finishes so list UIs can drop it.
    fn finish_item(&self, item: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.item = Some(item.to_string());
        inner.last_emit = std::time::Instant::now();
        drop(inner);
        self.emit();
    }

    fn finish(&self) {
        self.state_event(
            if self.cancelled() {
                "cancelled"
            } else {
                "done"
            },
            None,
        );
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
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(base);
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

pub fn plan_targets(
    paths: &[String],
    destination: &Path,
) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let destination = validate_directory(destination)?;
    let mut used = Vec::new();
    let mut planned = Vec::new();
    for path in paths {
        let source = validate_existing(Path::new(path), ExpectedKind::Any)?;
        let base = source
            .file_name()
            .ok_or("invalid source path")?
            .to_string_lossy()
            .into_owned();
        let name = reserve_unique_name(&destination, &base, &mut used);
        planned.push((source, child_path(&destination, &name)?));
    }
    Ok(planned)
}

fn same_device(source: &Path, destination: &Path) -> bool {
    let source_device = fs::symlink_metadata(source)
        .ok()
        .and_then(|m| device_id(&m));
    let dest_device = fs::symlink_metadata(destination)
        .ok()
        .and_then(|m| device_id(&m));
    matches!((source_device, dest_device), (Some(a), Some(b)) if a == b)
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "delete_items", fields(sentry_op = "file.delete"))]
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
    let outcome = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let roots: Result<Vec<PathBuf>, String> = paths
            .iter()
            .map(|path| validate_existing(Path::new(path), ExpectedKind::Any))
            .collect();
        let roots = roots?;
        let is_cancelled = || token.is_cancelled();
        let label = format!("Delete {} items", roots.len());
        if permanent {
            // No pre-scan: scanning and unlinking overlap so removal starts on
            // the first directory read. The total grows as files are discovered.
            let progress = FileOpProgress::new(
                app,
                token.clone(),
                job_id,
                "delete",
                label,
                String::new(),
                0,
                0,
            );
            let mut complete = true;
            for root in &roots {
                complete &= coordinated_write(root, || {
                    Ok(delete_tree_streaming(
                        std::slice::from_ref(root),
                        &is_cancelled,
                        |discovered| progress.set_total(discovered),
                        |removed| progress.add_files(removed),
                    ))
                })
                .unwrap_or(false);
            }
            if is_cancelled() || complete {
                progress.finish();
            } else {
                progress.fail("Some items could not be deleted".into());
            }
        } else {
            // Trash moves whole roots atomically, so there is nothing to measure
            // first: start the move immediately and count roots as items.
            let progress = FileOpProgress::new(
                app,
                token.clone(),
                job_id,
                "delete",
                label,
                String::new(),
                roots.len() as u64,
                0,
            );
            let ok = Mutex::new(true);
            run_workers(roots, &is_cancelled, |root| {
                let result =
                    coordinated_write(&root, || crate::explorer::file_ops::trash_local_item(&root));
                match result {
                    Ok(()) => progress
                        .finish_file(&root.file_name().unwrap_or_default().to_string_lossy()),
                    Err(_) => *ok.lock().unwrap() = false,
                }
            });
            if is_cancelled() || ok.into_inner().unwrap() {
                progress.finish();
            } else {
                progress.fail("Some items could not be moved to the Trash".into());
            }
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?;
    transfers.remove(&remove_id);
    outcome
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "copy_items", fields(sentry_op = "file.copy"))]
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
    let entries =
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<DirectoryEntry>, String> {
            let is_cancelled = || token.is_cancelled();
            // Plan every tree once: creates the directory skeleton on the target
            // and yields the exact file/byte totals without a second walk.
            let mut groups: Vec<(PathBuf, PathBuf, Vec<FileTask>)> = Vec::new();
            let mut complete = true;
            for (source, target) in &planned {
                let device = fs::symlink_metadata(source)
                    .ok()
                    .and_then(|m| device_id(&m));
                match coordinated_write(target, || {
                    Ok(plan_copy_tree(source, target, device, &is_cancelled))
                }) {
                    Ok((tasks, ok)) => {
                        complete &= ok;
                        groups.push((source.clone(), target.clone(), tasks));
                    }
                    Err(_) => complete = false,
                }
            }
            let files_total: u64 = groups.iter().map(|(_, _, tasks)| tasks.len() as u64).sum();
            let bytes_total: u64 = groups
                .iter()
                .flat_map(|(_, _, tasks)| tasks.iter())
                .map(|task| task.size)
                .sum();
            let progress = FileOpProgress::new(
                app.clone(),
                token.clone(),
                job_id,
                "copy",
                format!("Copy {} items", planned.len()),
                destination.clone(),
                files_total,
                bytes_total,
            );
            let produced = Mutex::new(Vec::<DirectoryEntry>::new());
            for (source, target, tasks) in groups {
                let result = coordinated_read(&source, || {
                    coordinated_write(&target, || {
                        if !copy_file_tasks(
                            tasks,
                            &is_cancelled,
                            |bytes| progress.add_bytes(bytes),
                            || progress.finish_file(&target.to_string_lossy()),
                        ) {
                            return Err("Some files could not be copied".to_string());
                        }
                        single_entry(&target)
                    })
                });
                match result {
                    Ok(entry) => {
                        produced.lock().unwrap().push(entry);
                        progress.finish_item(&source.to_string_lossy());
                    }
                    Err(_) => complete = false,
                }
            }
            if is_cancelled() || complete {
                progress.finish();
            } else {
                progress.fail("Some items could not be copied".into());
            }
            Ok(produced.into_inner().unwrap())
        })
        .await
        .map_err(|error| error.to_string())??;
    transfers.remove(&remove_id);
    for entry in &entries {
        let _ = insert_recent(
            database.inner(),
            &entry.path,
            &entry.name,
            &recent_kind(entry.is_directory),
        )
        .await;
    }
    Ok(entries)
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "move_items", fields(sentry_op = "file.move"))]
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
    let entries =
        tauri::async_runtime::spawn_blocking(move || -> Result<Vec<DirectoryEntry>, String> {
            let is_cancelled = || token.is_cancelled();
            let label = format!("Move {} items", planned.len());
            let produced = Mutex::new(Vec::<DirectoryEntry>::new());
            if planned.iter().all(|(source, _)| same_device(source, &dest)) {
                // Same volume: a rename per item is atomic and needs no walk.
                let mut complete = true;
                let progress = FileOpProgress::new(
                    app.clone(),
                    token.clone(),
                    job_id,
                    "move",
                    label,
                    destination.clone(),
                    planned.len() as u64,
                    0,
                );
                for (source, target) in &planned {
                    let result = coordinated_write(source, || {
                        coordinated_write(target, || {
                            fs::rename(source, target).map_err(|error| error.to_string())?;
                            progress.finish_file(&target.to_string_lossy());
                            single_entry(target)
                        })
                    });
                    match result {
                        Ok(entry) => {
                            produced.lock().unwrap().push(entry);
                            progress.finish_item(&source.to_string_lossy());
                        }
                        Err(_) => complete = false,
                    }
                }
                if is_cancelled() || complete {
                    progress.finish();
                } else {
                    progress.fail("Some items could not be moved".into());
                }
            } else {
                // Cross volume: plan once for exact totals, then copy and delete.
                let mut complete = true;
                let mut groups: Vec<(PathBuf, PathBuf, Vec<FileTask>)> = Vec::new();
                for (source, target) in &planned {
                    let device = fs::symlink_metadata(source)
                        .ok()
                        .and_then(|m| device_id(&m));
                    match coordinated_write(target, || {
                        Ok(plan_copy_tree(source, target, device, &is_cancelled))
                    }) {
                        Ok((tasks, ok)) => {
                            complete &= ok;
                            groups.push((source.clone(), target.clone(), tasks));
                        }
                        Err(_) => complete = false,
                    }
                }
                let files_total: u64 = groups.iter().map(|(_, _, tasks)| tasks.len() as u64).sum();
                let bytes_total: u64 = groups
                    .iter()
                    .flat_map(|(_, _, tasks)| tasks.iter())
                    .map(|task| task.size)
                    .sum();
                let progress = FileOpProgress::new(
                    app.clone(),
                    token.clone(),
                    job_id,
                    "move",
                    label,
                    destination.clone(),
                    files_total,
                    bytes_total,
                );
                for (source, target, tasks) in groups {
                    let result = coordinated_write(&source, || {
                        coordinated_write(&target, || {
                            let copied = copy_file_tasks(
                                tasks,
                                &is_cancelled,
                                |bytes| progress.add_bytes(bytes),
                                || {},
                            );
                            let removed = copied
                                && delete_tree_parallel(
                                    std::slice::from_ref(&source),
                                    &is_cancelled,
                                    || {},
                                );
                            if removed {
                                progress.finish_file(&target.to_string_lossy());
                            }
                            if !removed {
                                return Err("Some items could not be moved".to_string());
                            }
                            single_entry(&target)
                        })
                    });
                    match result {
                        Ok(entry) => {
                            produced.lock().unwrap().push(entry);
                            progress.finish_item(&source.to_string_lossy());
                        }
                        Err(_) => complete = false,
                    }
                }
                if is_cancelled() || complete {
                    progress.finish();
                } else {
                    progress.fail("Some items could not be moved".into());
                }
            }
            Ok(produced.into_inner().unwrap())
        })
        .await
        .map_err(|error| error.to_string())??;
    transfers.remove(&remove_id);
    for entry in &entries {
        let _ = insert_recent(
            database.inner(),
            &entry.path,
            &entry.name,
            &recent_kind(entry.is_directory),
        )
        .await;
    }
    Ok(entries)
}

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
        assert_eq!(
            (missing.files, missing.bytes, missing.complete),
            (0, 0, false)
        );

        fs::remove_dir_all(root).unwrap();
    }

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

    #[test]
    fn delete_tree_streaming_removes_files_and_directories() {
        let root = temp_dir("stream-delete");
        fs::create_dir_all(root.join("nested/empty")).unwrap();
        fs::write(root.join("one"), [0; 3]).unwrap();
        fs::write(root.join("nested/two"), [0; 7]).unwrap();
        let discovered = AtomicU64::new(0);
        let removed = AtomicU64::new(0);
        let complete = delete_tree_streaming(
            &[root.clone()],
            &never,
            |count| discovered.store(count, Ordering::Relaxed),
            |count| {
                removed.fetch_add(count, Ordering::Relaxed);
            },
        );
        assert!(complete);
        assert_eq!(discovered.load(Ordering::Relaxed), 2);
        assert_eq!(removed.load(Ordering::Relaxed), 2);
        assert!(!root.exists());
    }

    #[test]
    fn delete_tree_streaming_handles_many_small_files() {
        let root = temp_dir("stream-many");
        fs::create_dir_all(root.join("bulk")).unwrap();
        for index in 0..5000 {
            fs::write(root.join("bulk").join(format!("f-{index}")), b"x").unwrap();
        }
        let removed = AtomicU64::new(0);
        let complete = delete_tree_streaming(
            &[root.clone()],
            &never,
            |_| {},
            |count| {
                removed.fetch_add(count, Ordering::Relaxed);
            },
        );
        assert!(complete);
        assert_eq!(removed.load(Ordering::Relaxed), 5000);
        assert!(!root.exists());
    }

    #[test]
    fn delete_tree_streaming_reports_incomplete_when_cancelled() {
        let root = temp_dir("stream-cancel");
        fs::create_dir_all(&root).unwrap();
        for index in 0..8 {
            fs::write(root.join(format!("file-{index}")), [0; 1]).unwrap();
        }
        let complete = delete_tree_streaming(&[root.clone()], &|| true, |_| {}, |_| {});
        assert!(!complete);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn delete_tree_streaming_removes_symlinks_without_following() {
        let root = temp_dir("stream-link");
        let outside = temp_dir("stream-outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep"), [0; 4]).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.join("keep"), root.join("link")).unwrap();
            let complete = delete_tree_streaming(&[root.clone()], &never, |_| {}, |_| {});
            assert!(complete);
            assert!(!root.exists());
            assert!(outside.join("keep").exists());
        }
        fs::remove_dir_all(outside).unwrap();
    }

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
}
