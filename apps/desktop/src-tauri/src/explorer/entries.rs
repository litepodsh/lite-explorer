use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{ipc::Channel, State};

use crate::app::db::Database;
use crate::explorer::local_path::validate_directory;
use crate::explorer::paths::expand_tilde;
use crate::{network, remote, search};

#[derive(Serialize, Clone)]
pub struct DirectoryEntry {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) is_directory: bool,
    pub(crate) is_hidden: bool,
    pub(crate) size: Option<u64>,
    pub(crate) created: Option<u64>,
    pub(crate) modified: Option<u64>,
    /// Set for entries that aren't plain files or folders, like `"bucket"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) kind: Option<&'static str>,
}

const PROGRESSIVE_LISTING_THRESHOLD: usize = 1_000;
const PROGRESSIVE_LISTING_BATCH: usize = 10_000;

fn is_progressive_listing(count: usize) -> bool {
    count > PROGRESSIVE_LISTING_THRESHOLD
}

pub struct DirectoryListingScans(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl DirectoryListingScans {
    fn register(&self, request_id: &str) -> Option<Arc<AtomicBool>> {
        let mut scans = self.0.lock().unwrap();
        if scans.contains_key(request_id) {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        scans.insert(request_id.to_string(), Arc::clone(&cancelled));
        Some(cancelled)
    }

    fn unregister(&self, request_id: &str) {
        self.0.lock().unwrap().remove(request_id);
    }

    fn cancel(&self, request_id: &str) {
        if let Some(cancelled) = self.0.lock().unwrap().get(request_id) {
            cancelled.store(true, Ordering::Relaxed);
        }
    }
}

impl Default for DirectoryListingScans {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

fn listing_scans() -> &'static DirectoryListingScans {
    static SCANS: OnceLock<DirectoryListingScans> = OnceLock::new();
    SCANS.get_or_init(DirectoryListingScans::default)
}

#[derive(Serialize, Clone)]
pub struct DirectoryListingProgress {
    entries: Vec<DirectoryEntry>,
    done: bool,
    error: Option<String>,
}

#[cfg(target_os = "macos")]
pub fn coordinated_read<T>(
    path: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    use std::{cell::RefCell, rc::Rc};

    use block2::StackBlock;
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSError, NSFileCoordinator, NSFileCoordinatorReadingOptions, NSString, NSURL,
    };

    let path = path
        .to_str()
        .ok_or_else(|| "Path is not valid UTF-8".to_string())?
        .to_string();
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
        let result = Rc::new(RefCell::new(None));
        let action = Rc::new(RefCell::new(Some(action)));
        let block = StackBlock::new({
            let result = Rc::clone(&result);
            let action = Rc::clone(&action);
            move |_| *result.borrow_mut() = action.borrow_mut().take().map(|action| action())
        });
        let mut error: Option<objc2::rc::Retained<NSError>> = None;
        NSFileCoordinator::new().coordinateReadingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorReadingOptions::empty(),
            Some(&mut error),
            &block,
        );
        error.map_or_else(
            || {
                result
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| Err("File provider cancelled the read".into()))
            },
            |error| Err(error.to_string()),
        )
    })
}

#[cfg(not(target_os = "macos"))]
pub fn coordinated_read<T>(
    _: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    action()
}

#[cfg(target_os = "macos")]
pub fn coordinated_write<T>(
    path: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    use std::{cell::RefCell, rc::Rc};

    use block2::StackBlock;
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSError, NSFileCoordinator, NSFileCoordinatorWritingOptions, NSString, NSURL,
    };

    let path = path
        .to_str()
        .ok_or_else(|| "Path is not valid UTF-8".to_string())?
        .to_string();
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
        let result = Rc::new(RefCell::new(None));
        let action = Rc::new(RefCell::new(Some(action)));
        let block = StackBlock::new({
            let result = Rc::clone(&result);
            let action = Rc::clone(&action);
            move |_| *result.borrow_mut() = action.borrow_mut().take().map(|action| action())
        });
        let mut error: Option<objc2::rc::Retained<NSError>> = None;
        NSFileCoordinator::new().coordinateWritingItemAtURL_options_error_byAccessor(
            &url,
            NSFileCoordinatorWritingOptions::empty(),
            Some(&mut error),
            &block,
        );
        error.map_or_else(
            || {
                result
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| Err("File provider cancelled the write".into()))
            },
            |error| Err(error.to_string()),
        )
    })
}

#[cfg(not(target_os = "macos"))]
pub fn coordinated_write<T>(
    _: &Path,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    action()
}

pub fn directory_entries(path: &Path) -> Result<Vec<DirectoryEntry>, String> {
    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let entries: Vec<DirectoryEntry> = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let is_directory = entry.file_type().ok()?.is_dir();
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = entry.metadata().ok()?;
            Some(DirectoryEntry {
                is_hidden: name.starts_with('.'),
                name,
                path: entry.path().to_string_lossy().into_owned(),
                is_directory,
                size: (!is_directory).then(|| metadata.len()),
                created: epoch_millis(metadata.created()),
                modified: epoch_millis(metadata.modified()),
                kind: None,
            })
        })
        .collect();
    Ok(entries)
}

fn send_listing(
    on_progress: &Channel<DirectoryListingProgress>,
    entries: Vec<DirectoryEntry>,
    done: bool,
    error: Option<String>,
) -> bool {
    on_progress
        .send(DirectoryListingProgress {
            entries,
            done,
            error,
        })
        .is_ok()
}

fn stream_directory_entries(
    path: &Path,
    cancelled: &AtomicBool,
    on_progress: &Channel<DirectoryListingProgress>,
) -> Result<(), String> {
    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let mut entries = Vec::with_capacity(PROGRESSIVE_LISTING_BATCH);
    let mut progressive = false;
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entry) = entry else { continue };
        let is_directory = match entry.file_type() {
            Ok(kind) => kind.is_dir(),
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        entries.push(DirectoryEntry {
            is_hidden: name.starts_with('.'),
            name,
            path: entry.path().to_string_lossy().into_owned(),
            is_directory,
            size: (!is_directory).then(|| metadata.len()),
            created: epoch_millis(metadata.created()),
            modified: epoch_millis(metadata.modified()),
            kind: None,
        });
        let just_became_progressive = !progressive && is_progressive_listing(entries.len());
        if just_became_progressive {
            progressive = true;
        }
        if just_became_progressive || (progressive && entries.len() >= PROGRESSIVE_LISTING_BATCH) {
            if !send_listing(on_progress, std::mem::take(&mut entries), false, None) {
                break;
            }
        }
    }
    if !cancelled.load(Ordering::Relaxed) {
        send_listing(on_progress, entries, true, None);
    }
    Ok(())
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "read_directory", fields(sentry_op = "file.list"))]
pub async fn read_directory(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    mounts: State<'_, network::Mounts>,
    path: String,
) -> Result<Vec<DirectoryEntry>, String> {
    if network::servers::is_server_path(&path) {
        return network::servers::list_directory(&database.0, &sessions, &path).await;
    }
    if network::is_network_path(&path) {
        return network::list_shares(&database.0, &mounts, &path).await;
    }
    if remote::is_remote_path(&path) {
        return remote::list_directory(&database.0, &clients, &path).await;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_directory(&expand_tilde(&path))?;
        coordinated_read(&path, || directory_entries(&path))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
#[tracing::instrument(
    skip_all,
    name = "read_directory_progressively",
    fields(sentry_op = "file.list")
)]
pub fn read_directory_progressively(
    path: String,
    request_id: String,
    on_progress: Channel<DirectoryListingProgress>,
) -> Result<(), String> {
    let path = validate_directory(&expand_tilde(&path))?;
    let cancelled = listing_scans()
        .register(&request_id)
        .ok_or("Duplicate directory listing request")?;
    std::thread::spawn(move || {
        let result = coordinated_read(&path, || {
            stream_directory_entries(&path, &cancelled, &on_progress)
        });
        if let Err(error) = result {
            send_listing(&on_progress, Vec::new(), true, Some(error));
        }
        listing_scans().unregister(&request_id);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_directory_listing(request_id: String) {
    listing_scans().cancel(&request_id);
}

/// Expands a leading `~` and returns the absolute local path.
#[tauri::command]
pub fn resolve_path(path: String) -> String {
    expand_tilde(&path).to_string_lossy().into_owned()
}

/// Whether a local path already exists, for pre-flight checks like naming an archive.
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).exists()
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "search_directory", fields(sentry_op = "file.search"))]
pub async fn search_directory(
    path: String,
    query: String,
    mode: search::SearchMode,
) -> Result<search::SearchResponse, String> {
    if remote::is_remote_path(&path)
        || network::is_network_path(&path)
        || network::servers::is_server_path(&path)
    {
        return Err("Search is available for local folders and mounted volumes only.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_directory(&PathBuf::from(path))?;
        search::search_directory(&path, &query, mode)
    })
    .await
    .map_err(|error| error.to_string())?
}

pub fn single_entry(path: &Path) -> Result<DirectoryEntry, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(DirectoryEntry {
        is_hidden: name.starts_with('.'),
        name,
        path: path.to_string_lossy().into_owned(),
        is_directory: metadata.is_dir(),
        size: None,
        created: epoch_millis(metadata.created()),
        modified: epoch_millis(metadata.modified()),
        kind: None,
    })
}

pub fn unique_name(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
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
        let numbered = format!("{stem}_{index}{extension}");
        if !parent.join(&numbered).exists() {
            return numbered;
        }
        index += 1;
    }
}

pub fn epoch_millis(time: std::io::Result<SystemTime>) -> Option<u64> {
    time.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

pub fn utf8_boundary(bytes: &[u8], limit: usize) -> usize {
    let mut end = limit.min(bytes.len());
    while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn directory_entries_uses_file_type_without_metadata() {
        let directory = std::env::temp_dir().join(format!(
            "liteexplorer-directory-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(directory.join("folder")).unwrap();
        fs::write(directory.join("file.txt"), "test").unwrap();
        fs::write(directory.join(".hidden"), "test").unwrap();

        let entries = directory_entries(&directory).unwrap();

        assert!(entries
            .iter()
            .any(|entry| entry.name == "folder" && entry.is_directory));
        assert!(entries.iter().any(|entry| entry.name == "file.txt"
            && !entry.is_directory
            && !entry.is_hidden
            && entry.size == Some(4)));
        assert!(entries
            .iter()
            .any(|entry| entry.name == ".hidden" && entry.is_hidden));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn large_listings_start_after_one_thousand_entries() {
        assert!(!is_progressive_listing(1_000));
        assert!(is_progressive_listing(1_001));
    }

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn coordinates_local_file_provider_reads() {
        let path = std::env::temp_dir().join(format!("liteexplorer-coordination-{}", nanos()));
        fs::write(&path, "coordinated").unwrap();

        let text = coordinated_read(&path, || {
            fs::read_to_string(&path).map_err(|error| error.to_string())
        })
        .unwrap();

        assert_eq!(text, "coordinated");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn unique_name_numbers_before_the_extension() {
        let parent = std::env::temp_dir().join(format!("liteexplorer-unique-{}", nanos()));
        fs::create_dir_all(&parent).unwrap();
        fs::write(parent.join("checklist.md"), "one").unwrap();
        fs::write(parent.join("checklist_2.md"), "two").unwrap();
        assert_eq!(unique_name(&parent, "checklist.md"), "checklist_3.md");
        assert_eq!(unique_name(&parent, "archive.tar.gz"), "archive.tar.gz");
        fs::write(parent.join("archive.tar.gz"), "one").unwrap();
        assert_eq!(unique_name(&parent, "archive.tar.gz"), "archive.tar_2.gz");
        fs::remove_dir_all(parent).unwrap();
    }
}
