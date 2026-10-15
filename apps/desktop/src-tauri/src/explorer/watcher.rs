//! Watches a directory and emits incremental change events so the UI can patch
//! its listing instead of re-reading. Per pane we keep one watcher rooted at
//! the folder it shows. `notify` delivers raw events; the debouncer collapses
//! bursty editor writes (the "save creates tmp + rename" pattern) into one
//! batch we then reconcile against an in-memory `(mtime, size)` cache.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, FileIdMap,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::explorer::entries::{coordinated_read, epoch_millis};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntryStamp {
    mtime: u64,
    size: u64,
}

/// One pane's worth of cache + debouncer. Dropped when the pane navigates away.
pub struct WatcherSession {
    /// Keyed by absolute path. Only direct children of the root are tracked.
    entries: Mutex<HashMap<PathBuf, EntryStamp>>,
    /// Files we've already reported as removed in the current batch, so a
    /// rename's `From` side and the subsequent `To` side merge cleanly.
    pending_removals: Mutex<HashSet<PathBuf>>,
    /// Root this session was created for. Used to filter foreign events.
    root: PathBuf,
    /// Owns the debouncer; stops watching when dropped.
    _debouncer: Debouncer<RecommendedWatcher, FileIdMap>,
    /// Channel handle for emitting events to the frontend.
    app: AppHandle,
    /// Stable id the frontend uses to route events to the right pane.
    session_id: String,
}

/// Registry of active sessions, keyed by `session_id` (frontend-chosen).
#[derive(Default)]
pub struct WatcherRegistry {
    inner: Mutex<HashMap<String, Arc<WatcherSession>>>,
}

impl WatcherRegistry {
    pub fn get(&self, session_id: &str) -> Option<Arc<WatcherSession>> {
        self.inner.lock().unwrap().get(session_id).cloned()
    }

    pub fn insert(&self, session_id: String, session: Arc<WatcherSession>) {
        self.inner.lock().unwrap().insert(session_id, session);
    }

    pub fn remove(&self, session_id: &str) -> Option<Arc<WatcherSession>> {
        self.inner.lock().unwrap().remove(session_id)
    }

    pub fn contains(&self, session_id: &str) -> bool {
        self.inner.lock().unwrap().contains_key(session_id)
    }
}

fn registry() -> &'static WatcherRegistry {
    static REGISTRY: OnceLock<WatcherRegistry> = OnceLock::new();
    REGISTRY.get_or_init(WatcherRegistry::default)
}

/// Payload sent to the frontend. `entry` is `None` for `Removed`.
#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WatchEvent {
    Added { entry: DirectoryEntryView },
    Modified { entry: DirectoryEntryView },
    Removed { path: String },
}

/// Minimal entry payload — enough to insert/update a row, no measured size
/// (the frontend already carries its own `size`/`sizeComplete` per row).
#[derive(Serialize, Clone)]
pub struct DirectoryEntryView {
    name: String,
    path: String,
    is_directory: bool,
    is_hidden: bool,
    modified: Option<u64>,
    created: Option<u64>,
    kind: Option<&'static str>,
}

impl DirectoryEntryView {
    fn from_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let is_directory = metadata.is_dir();
        Some(Self {
            is_hidden: name.starts_with('.'),
            name,
            path: path.to_string_lossy().into_owned(),
            is_directory,
            created: epoch_millis(metadata.created()),
            modified: epoch_millis(metadata.modified()),
            kind: None,
        })
    }
}

/// Result of one debounced event batch, used by tests and the manual
/// `reconcile_directory` command.
#[derive(Default, Serialize)]
pub struct ReconcileStats {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
}

/// Pure variant of `reconcile` that returns the deltas instead of emitting.
/// Used by tests so they don't require the Tauri runtime.
#[allow(dead_code)]
pub(crate) fn reconcile_stats(
    root: &Path,
    entries: &mut HashMap<PathBuf, EntryStamp>,
    pending_removals: &mut HashSet<PathBuf>,
) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    let Ok(read) = fs::read_dir(root) else {
        return stats;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        seen.insert(path.clone());
        let Ok(metadata) = entry.metadata() else { continue };
        let Some(mtime) = epoch_millis(metadata.modified()) else {
            continue;
        };
        let stamp = EntryStamp { mtime, size: metadata.len() };
        match entries.get(&path).copied() {
            None => {
                if DirectoryEntryView::from_path(&path).is_some() {
                    stats.added += 1;
                }
                entries.insert(path, stamp);
            }
            Some(prev) if prev != stamp => {
                if DirectoryEntryView::from_path(&path).is_some() {
                    stats.modified += 1;
                }
                entries.insert(path, stamp);
            }
            _ => {}
        }
    }

    let removed: Vec<PathBuf> = entries
        .keys()
        .filter(|path| !seen.contains(path.as_path()))
        .cloned()
        .collect();
    for path in removed {
        entries.remove(&path);
        if pending_removals.insert(path) {
            stats.removed += 1;
        }
    }
    stats
}

/// Walks `root` once and records `(mtime, size)` for every direct child.
/// Symlinks are resolved for `is_dir()` decisions via `fs::metadata`.
pub(crate) fn build_stamps(root: &Path) -> HashMap<PathBuf, EntryStamp> {
    let mut stamps = HashMap::new();
    let Ok(read) = fs::read_dir(root) else {
        return stamps;
    };
    for entry in read.filter_map(Result::ok) {
        let Ok(metadata) = entry.metadata() else { continue };
        let Some(mtime) = epoch_millis(metadata.modified()) else {
            continue;
        };
        let size = metadata.len();
        stamps.insert(entry.path(), EntryStamp { mtime, size });
    }
    stamps
}

/// Emits one event per observed delta. Shared between the watcher callback
/// and `reconcile_directory` so the rules for "what counts as a change" live
/// in one place.
pub(crate) fn reconcile(
    root: &Path,
    entries: &mut HashMap<PathBuf, EntryStamp>,
    pending_removals: &mut HashSet<PathBuf>,
    app: &AppHandle,
    session_id: &str,
) -> ReconcileStats {
    let mut stats = ReconcileStats::default();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut added: Vec<(PathBuf, DirectoryEntryView, EntryStamp)> = Vec::new();

    let Ok(read) = fs::read_dir(root) else {
        return stats;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        seen.insert(path.clone());
        let Ok(metadata) = entry.metadata() else { continue };
        let Some(mtime) = epoch_millis(metadata.modified()) else {
            continue;
        };
        let stamp = EntryStamp { mtime, size: metadata.len() };
        match entries.get(&path).copied() {
            None => {
                if let Some(view) = DirectoryEntryView::from_path(&path) {
                    added.push((path, view, stamp));
                }
            }
            Some(prev) if prev != stamp => {
                if let Some(view) = DirectoryEntryView::from_path(&path) {
                    let _ = app.emit(
                        &event_name(session_id),
                        WatchEvent::Modified { entry: view },
                    );
                    stats.modified += 1;
                }
                entries.insert(path, stamp);
            }
            _ => {}
        }
    }

    // Emit removals first so the UI never shows both the old and the new
    // name at the same instant during an in-place rename. The entries we
    // flagged as `added` stay out of the cache until their event is on the
    // wire, so the next reconcile won't double-report them either.
    let removed: Vec<PathBuf> = entries
        .keys()
        .filter(|path| !seen.contains(path.as_path()))
        .cloned()
        .collect();
    for path in removed {
        entries.remove(&path);
        // Deduplicate within a batch: the debouncer sometimes reports the
        // same removal twice when an editor does `unlink + rename`.
        if pending_removals.insert(path.clone()) {
            let _ = app.emit(
                &event_name(session_id),
                WatchEvent::Removed { path: path.to_string_lossy().into_owned() },
            );
            stats.removed += 1;
        }
    }

    for (path, view, stamp) in added {
        let _ = app.emit(&event_name(session_id), WatchEvent::Added { entry: view });
        stats.added += 1;
        entries.insert(path, stamp);
    }

    stats
}

fn event_name(session_id: &str) -> String {
    format!("watcher://{session_id}")
}

fn handle_notify_event(session: &WatcherSession, result: DebounceEventResult) {
    let Ok(events) = result else {
        return;
    };

    let mut entries = session.entries.lock().unwrap();
    let mut pending = session.pending_removals.lock().unwrap();

    // Translate the raw events first. `notify` is the source of truth for
    // "a file left `root`" — when Finder moves a file to the Trash, the
    // reconciliation scan can still see the inode if it races the rename,
    // so trusting the debouncer's report is safer for removals.
    for event in &events {
        translate_event(
            event,
            &session.root,
            &mut entries,
            &mut pending,
            &session.app,
            &session.session_id,
        );
    }
    // One reconcile afterwards catches the cases the raw events don't cover
    // (renames inside `root`, metadata changes, missed events from sandboxing).
    reconcile(
        &session.root,
        &mut entries,
        &mut pending,
        &session.app,
        &session.session_id,
    );
}

/// Emits `Removed` for paths the kernel reported as gone from `root`, and
/// keeps the in-memory stamps consistent so the trailing `reconcile` doesn't
/// double-report. Other event kinds are ignored — the reconcile pass handles
/// them.
fn translate_event(
    event: &DebouncedEvent,
    root: &Path,
    entries: &mut HashMap<PathBuf, EntryStamp>,
    pending_removals: &mut HashSet<PathBuf>,
    app: &AppHandle,
    session_id: &str,
) {
    let kind = event.event.kind;
    let paths = &event.event.paths;
    match kind {
        EventKind::Remove(_) => {
            for path in paths {
                emit_remove(root, path, entries, pending_removals, app, session_id);
            }
        }
        EventKind::Modify(ModifyKind::Name(ref mode)) => {
            // `Name(From)` arrives both for renames inside `root` and for
            // moves out of `root` (Finder → Trash). We can only tell them
            // apart by checking whether the `To` side is also under `root`:
            // `Both` events carry both paths in `paths[1]`; pure `From` and
            // `Other` modes leave it empty. When we can't be sure, skip and
            // let the trailing reconcile figure it out — emitting `Removed`
            // for a rename would drop the row only to have it come back as
            // `Added`, which is the bug we already saw.
            //
            // `Other` is the imprecise-mode catch-all; treat it like `Both`
            // when we have a sibling path to inspect.
            let sibling = event.event.paths.get(1);
            let to_in_root = sibling.is_some_and(|to| to.parent() == Some(root));
            let is_external = match mode {
                RenameMode::From | RenameMode::Other => !to_in_root,
                _ => false,
            };
            if is_external {
                for path in paths {
                    emit_remove(root, path, entries, pending_removals, app, session_id);
                }
            }
        }
        _ => {}
    }
}

fn emit_remove(
    root: &Path,
    path: &Path,
    entries: &mut HashMap<PathBuf, EntryStamp>,
    pending_removals: &mut HashSet<PathBuf>,
    app: &AppHandle,
    session_id: &str,
) {
    // Only direct children of `root` are tracked; nested paths belong to a
    // different pane.
    let parent = match path.parent() {
        Some(parent) => parent,
        None => return,
    };
    if parent != root {
        return;
    }
    entries.remove(path);
    if pending_removals.insert(path.to_path_buf()) {
        let _ = app.emit(
            &event_name(session_id),
            WatchEvent::Removed { path: path.to_string_lossy().into_owned() },
        );
    }
}

/// Starts a watcher for `root` and registers it under `session_id`. Returns
/// `Err` if the path is not a directory.
pub(crate) fn start_session(
    app: AppHandle,
    session_id: String,
    root: PathBuf,
) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    if registry().contains(&session_id) {
        // Re-starting with the same id is a no-op; the frontend just re-asserts.
        return Ok(());
    }

    // First read under the macOS file coordinator so iCloud/Dropbox paths
    // don't return a stale snapshot mid-sync.
    let entries = coordinated_read(&root, || Ok(build_stamps(&root)))
        .unwrap_or_default();

    let (tx, rx) = std::sync::mpsc::channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(
        std::time::Duration::from_millis(500),
        None,
        move |result: DebounceEventResult| {
            let _ = tx.send(result);
        },
    )
    .map_err(|error| error.to_string())?;
    debouncer
        .watcher()
        .watch(&root, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;
    // The cache uses absolute paths, so child renames across subdirs would
    // miss. NonRecursive is fine: panes show one folder, not its tree.
    debouncer
        .cache()
        .add_root(&root, RecursiveMode::NonRecursive);

    let session = Arc::new(WatcherSession {
        entries: Mutex::new(entries),
        pending_removals: Mutex::new(HashSet::new()),
        root: root.clone(),
        _debouncer: debouncer,
        app,
        session_id,
    });
    let thread_session = Arc::clone(&session);
    std::thread::Builder::new()
        .name("liteexplorer-fs-watcher".into())
        .spawn(move || {
            for result in rx {
                handle_notify_event(&thread_session, result);
            }
        })
        .map_err(|error| error.to_string())?;
    registry().insert(session.session_id.clone(), session);
    Ok(())
}

/// Stops the watcher for `session_id` if one is running.
pub(crate) fn stop_session(session_id: &str) {
    if let Some(_session) = registry().remove(session_id) {
        // Dropping the Arc stops the debouncer's watcher thread.
    }
}

/// Diagnostic helper exposed as a Tauri command so the frontend can pull a
/// fresh reconciliation when it suspects drift (e.g. after waking from sleep).
#[tauri::command]
pub fn reconcile_directory(session_id: String) -> ReconcileStats {
    let Some(session) = registry().get(&session_id) else {
        return ReconcileStats::default();
    };
    let mut entries = session.entries.lock().unwrap();
    let mut pending = session.pending_removals.lock().unwrap();
    reconcile(&session.root, &mut entries, &mut pending, &session.app, &session_id)
}

/// Tauri command: start watching `path` for the pane identified by `session_id`.
#[tauri::command]
pub fn watch_directory(app: AppHandle, session_id: String, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    start_session(app, session_id, path)
}

/// Tauri command: stop watching the session. Safe to call with an unknown id.
#[tauri::command]
pub fn unwatch_directory(session_id: String) {
    stop_session(&session_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nanos() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn build_stamps_lists_direct_children_only() {
        let root = std::env::temp_dir().join(format!("liteexplorer-stamps-{}", nanos()));
        fs::create_dir_all(root.join("sub/inner")).unwrap();
        fs::write(root.join("sub/inner/deep.txt"), "hi").unwrap();
        fs::write(root.join("file.txt"), "hi").unwrap();
        let stamps = build_stamps(&root);
        assert!(stamps.contains_key(&root.join("file.txt")));
        assert!(stamps.contains_key(&root.join("sub")), "subdirs are listed as entries");
        assert!(
            !stamps.contains_key(&root.join("sub/inner/deep.txt")),
            "must skip nested files"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconcile_detects_add_and_remove() {
        let root = std::env::temp_dir().join(format!("liteexplorer-reconcile-{}", nanos()));
        fs::create_dir_all(&root).unwrap();
        let mut entries = build_stamps(&root);
        let mut pending = HashSet::new();

        fs::write(root.join("a.txt"), "a").unwrap();
        let stats = reconcile_stats(&root, &mut entries, &mut pending);
        assert_eq!(stats.added, 1);

        fs::remove_file(root.join("a.txt")).unwrap();
        let stats = reconcile_stats(&root, &mut entries, &mut pending);
        assert_eq!(stats.removed, 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reconcile_dedupes_duplicate_removals() {
        let root = std::env::temp_dir().join(format!("liteexplorer-reconcile-dedup-{}", nanos()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        let mut entries = build_stamps(&root);
        let mut pending = HashSet::new();
        fs::remove_file(root.join("a.txt")).unwrap();
        let first = reconcile_stats(&root, &mut entries, &mut pending);
        // Second call with the same state must not re-report; the file is
        // already gone and the cache no longer mentions it.
        let second = reconcile_stats(&root, &mut entries, &mut pending);
        assert_eq!(first.removed, 1);
        assert_eq!(second.removed, 0);
        fs::remove_dir_all(root).unwrap();
    }
}