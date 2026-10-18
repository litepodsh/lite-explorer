//! Watches the folder each pane shows, so files other apps add, remove or rename
//! (a finished download, a saved document) appear without a manual refresh.
use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};

use notify_debouncer_mini::{
    new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
    Debouncer,
};
use tauri::{ipc::Channel, State};

/// Bursts of events (a download writing chunks, a copy of many files) become one refresh.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// One watcher per pane; more than this means panes leaked their watcher.
const MAX_WATCHERS: usize = 64;

#[derive(Default)]
pub struct FolderWatchers(Mutex<HashMap<String, (String, Debouncer<RecommendedWatcher>)>>);

/// Starts watching `path` for `pane_id`, replacing the folder that pane watched before.
/// Only the folder's own entries are watched, not its subfolders.
#[tauri::command]
pub async fn watch_directory(
    watchers: State<'_, FolderWatchers>,
    pane_id: String,
    path: String,
    on_change: Channel<()>,
) -> Result<(), String> {
    dbg_log(&format!("watch {pane_id} {path}"));
    let folder = Path::new(&path);
    if !folder.is_absolute() || !folder.is_dir() {
        return Err("Expected an absolute local folder".into());
    }
    let mut map = watchers.0.lock().unwrap();
    map.remove(&pane_id);
    if map.len() >= MAX_WATCHERS {
        return Err("Too many folders watched".into());
    }
    let mut debouncer = new_debouncer(
        DEBOUNCE,
        move |result: notify_debouncer_mini::DebounceEventResult| {
            dbg_log(&format!("event {result:?}"));
            if result.is_ok_and(|events| !events.is_empty()) {
                let sent = on_change.send(());
                dbg_log(&format!("sent {sent:?}"));
            }
        },
    )
    .map_err(|error| error.to_string())?;
    debouncer
        .watcher()
        .watch(folder, RecursiveMode::NonRecursive)
        .map_err(|error| error.to_string())?;
    map.insert(pane_id, (path, debouncer));
    Ok(())
}

/// Stops the watcher of `pane_id` if it still watches `path`: the pane may already watch
/// the next folder when this arrives. Dropping the debouncer stops its thread.
#[tauri::command]
pub fn unwatch_directory(watchers: State<'_, FolderWatchers>, pane_id: String, path: String) {
    dbg_log(&format!("unwatch {pane_id} {path}"));
    let mut map = watchers.0.lock().unwrap();
    if map
        .get(&pane_id)
        .is_some_and(|(watched, _)| *watched == path)
    {
        map.remove(&pane_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn new_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        // Canonical path: macOS reports /private/var for a /var temp folder.
        let folder = dir.path().canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let mut debouncer = new_debouncer(
            DEBOUNCE,
            move |result: notify_debouncer_mini::DebounceEventResult| {
                let _ = tx.send(result.is_ok());
            },
        )
        .unwrap();
        debouncer
            .watcher()
            .watch(&folder, RecursiveMode::NonRecursive)
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(folder.join("report.pdf"), b"done").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(true));
    }
}

// DEBUG-WATCH: temporary
fn dbg_log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/private/tmp/claude-501/-Users-sebastiangarzon-Work-lp-lite-explorer-apps-desktop/49aec325-e185-4e49-b28b-0bc0fb29003c/scratchpad/watch.log") {
        let _ = writeln!(f, "{line}");
    }
}
