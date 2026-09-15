use std::{collections::HashSet, fs, path::Path};

use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app::db::Database;
use crate::explorer::paths::{home_location, now_secs};
use crate::system::trash::trash_bytes;
use crate::system::volumes::{allocated_bytes, device_id};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FolderUsageEntry {
    name: String,
    path: String,
    bytes: u64,
    is_hidden: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct FolderUsage {
    root: String,
    total_bytes: u64,
    scanned_at: Option<i64>,
    scanning: bool,
    entries: Vec<FolderUsageEntry>,
    /// Total size of the OS trash, summed across the locations for this platform.
    trash_bytes: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct FolderUsageProgress {
    root: String,
    scanned_bytes: u64,
    current: Option<String>,
    entry: Option<FolderUsageEntry>,
}

#[derive(Serialize, Clone)]
pub struct FolderUsageError {
    root: String,
    message: String,
}

/// Roots with a folder usage scan in flight, so entering the overview twice never starts two walks.
pub struct FolderScans(pub(crate) std::sync::Mutex<HashSet<String>>);

pub const SCAN_PROGRESS_EVERY_ENTRIES: u64 = 256;

/// Walks `path` without following symlinks or crossing into other volumes.
pub fn tree_usage(
    path: &Path,
    device: Option<u64>,
    seen: &mut HashSet<(u64, u64)>,
    on_progress: &mut dyn FnMut(u64),
) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    let mut total = allocated_bytes(&metadata, seen);
    if !metadata.is_dir() {
        return total;
    }
    let mut visited = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(read) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                if device.is_some() && device_id(&metadata) != device {
                    continue;
                }
                stack.push(entry.path());
            }
            total += allocated_bytes(&metadata, seen);
            visited += 1;
            if visited.is_multiple_of(SCAN_PROGRESS_EVERY_ENTRIES) {
                on_progress(total);
            }
        }
    }
    total
}

pub enum ScanEvent<'a> {
    /// A folder still being walked, with the bytes found in it so far.
    Progress {
        scanned_bytes: u64,
        entry: &'a FolderUsageEntry,
    },
    /// A folder whose walk finished.
    Entry {
        scanned_bytes: u64,
        entry: &'a FolderUsageEntry,
    },
}

/// Sizes every direct subfolder of `root`. Loose files only count toward the total.
pub fn scan_folder_usage_tree(
    root: &Path,
    report: &mut dyn FnMut(ScanEvent),
) -> (u64, Vec<FolderUsageEntry>) {
    let mut seen = HashSet::new();
    let root_metadata = fs::symlink_metadata(root).ok();
    let device = root_metadata.as_ref().and_then(device_id);
    let mut scanned = root_metadata
        .as_ref()
        .map_or(0, |metadata| allocated_bytes(metadata, &mut seen));
    let mut entries = Vec::new();
    let Ok(read) = fs::read_dir(root) else {
        return (scanned, entries);
    };
    for entry in read.filter_map(Result::ok) {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_dir() {
            scanned += allocated_bytes(&metadata, &mut seen);
            continue;
        }
        if device.is_some() && device_id(&metadata) != device {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let mut item = FolderUsageEntry {
            is_hidden: name.starts_with('.'),
            path: entry.path().to_string_lossy().into_owned(),
            name,
            bytes: 0,
        };
        let base = scanned;
        let bytes = tree_usage(&entry.path(), device, &mut seen, &mut |so_far| {
            item.bytes = so_far;
            report(ScanEvent::Progress {
                scanned_bytes: base + so_far,
                entry: &item,
            })
        });
        scanned = base + bytes;
        item.bytes = bytes;
        report(ScanEvent::Entry {
            scanned_bytes: scanned,
            entry: &item,
        });
        entries.push(item);
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    (scanned, entries)
}

pub fn usage_root(root: Option<String>) -> Result<String, String> {
    root.or_else(|| home_location().map(|home| home.path))
        .ok_or_else(|| "Couldn’t find your home folder.".into())
}

pub async fn save_folder_usage(
    pool: &SqlitePool,
    root: &str,
    total_bytes: u64,
    scanned_at: i64,
    trash_bytes: Option<u64>,
    entries: &[FolderUsageEntry],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM folder_usage WHERE root = ?")
        .bind(root)
        .execute(&mut *transaction)
        .await?;
    for entry in entries {
        sqlx::query(
            "INSERT INTO folder_usage (root, path, name, bytes, is_hidden) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(root)
        .bind(&entry.path)
        .bind(&entry.name)
        .bind(entry.bytes as i64)
        .bind(entry.is_hidden)
        .execute(&mut *transaction)
        .await?;
    }
    sqlx::query(
        "INSERT INTO folder_scans (root, total_bytes, scanned_at, trash_bytes) VALUES (?, ?, ?, ?) \
         ON CONFLICT(root) DO UPDATE SET total_bytes = excluded.total_bytes, scanned_at = excluded.scanned_at, trash_bytes = excluded.trash_bytes",
    )
    .bind(root)
    .bind(total_bytes as i64)
    .bind(scanned_at)
    .bind(trash_bytes.map(|bytes| bytes as i64))
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await
}

pub async fn load_folder_usage(
    pool: &SqlitePool,
    root: &str,
    scanning: bool,
) -> Result<FolderUsage, sqlx::Error> {
    let scan =
        sqlx::query("SELECT total_bytes, scanned_at, trash_bytes FROM folder_scans WHERE root = ?")
            .bind(root)
            .fetch_optional(pool)
            .await?;
    let entries = sqlx::query("SELECT name, path, bytes, is_hidden FROM folder_usage WHERE root = ? ORDER BY bytes DESC, name")
        .bind(root)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| FolderUsageEntry {
            name: row.get("name"),
            path: row.get("path"),
            bytes: row.get::<i64, _>("bytes") as u64,
            is_hidden: row.get("is_hidden"),
        })
        .collect();
    Ok(FolderUsage {
        root: root.to_owned(),
        total_bytes: scan
            .as_ref()
            .map_or(0, |row| row.get::<i64, _>("total_bytes") as u64),
        scanned_at: scan.as_ref().map(|row| row.get("scanned_at")),
        scanning,
        entries,
        trash_bytes: scan.and_then(|row| {
            row.get::<Option<i64>, _>("trash_bytes")
                .map(|bytes| bytes as u64)
        }),
    })
}

#[tauri::command]
pub async fn folder_usage(
    root: Option<String>,
    database: State<'_, Database>,
    scans: State<'_, FolderScans>,
) -> Result<FolderUsage, String> {
    let root = usage_root(root)?;
    let scanning = scans.0.lock().unwrap().contains(&root);
    load_folder_usage(&database.0, &root, scanning)
        .await
        .map_err(|error| error.to_string())
}

pub const SCAN_PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);

#[tauri::command]
pub fn scan_folder_usage(
    root: Option<String>,
    app: AppHandle,
    scans: State<'_, FolderScans>,
) -> Result<(), String> {
    let root = usage_root(root)?;
    if !scans.0.lock().unwrap().insert(root.clone()) {
        return Ok(());
    }
    std::thread::spawn(move || {
        let mut last_progress = std::time::Instant::now();
        let (total_bytes, entries) = scan_folder_usage_tree(Path::new(&root), &mut |event| {
            let progress = match event {
                ScanEvent::Progress {
                    scanned_bytes,
                    entry,
                } => {
                    if last_progress.elapsed() < SCAN_PROGRESS_INTERVAL {
                        return;
                    }
                    FolderUsageProgress {
                        root: root.clone(),
                        scanned_bytes,
                        current: Some(entry.name.clone()),
                        entry: Some(entry.clone()),
                    }
                }
                ScanEvent::Entry {
                    scanned_bytes,
                    entry,
                } => FolderUsageProgress {
                    root: root.clone(),
                    scanned_bytes,
                    current: None,
                    entry: Some(entry.clone()),
                },
            };
            last_progress = std::time::Instant::now();
            let _ = app.emit("folder-usage-progress", progress);
        });
        let scanned_at = now_secs();
        let trash = trash_bytes();
        let pool = app.state::<Database>().0.clone();
        let saved = tauri::async_runtime::block_on(save_folder_usage(
            &pool,
            &root,
            total_bytes,
            scanned_at,
            trash,
            &entries,
        ));
        app.state::<FolderScans>().0.lock().unwrap().remove(&root);
        match saved {
            Ok(()) => {
                let _ = app.emit(
                    "folder-usage-done",
                    FolderUsage {
                        root,
                        total_bytes,
                        scanned_at: Some(scanned_at),
                        scanning: false,
                        entries,
                        trash_bytes: trash,
                    },
                );
            }
            Err(error) => {
                let _ = app.emit(
                    "folder-usage-error",
                    FolderUsageError {
                        root,
                        message: format!("Couldn’t save folder sizes: {error}"),
                    },
                );
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn usage_fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "liteexplorer-usage-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("single/nested")).unwrap();
        fs::create_dir_all(root.join("linked/nested")).unwrap();
        fs::create_dir_all(root.join(".cache")).unwrap();
        fs::write(root.join("single/nested/data.bin"), vec![7u8; 64_000]).unwrap();
        fs::write(root.join("linked/nested/data.bin"), vec![7u8; 64_000]).unwrap();
        fs::hard_link(
            root.join("linked/nested/data.bin"),
            root.join("linked/nested/copy.bin"),
        )
        .unwrap();
        fs::write(root.join(".cache/blob"), vec![1u8; 8_000]).unwrap();
        fs::write(root.join("loose.txt"), vec![1u8; 8_000]).unwrap();
        root
    }

    #[test]
    fn folder_usage_scan_sizes_subfolders_recursively() {
        let root = usage_fixture();
        let mut entry_events = 0;
        let (total, entries) = scan_folder_usage_tree(&root, &mut |event| {
            if let ScanEvent::Entry { entry, .. } = event {
                assert!(entry.bytes > 0);
                entry_events += 1;
            }
        });

        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        let size = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.name == name)
                .unwrap()
                .bytes
        };
        assert_eq!(entry_events, 3);
        assert_eq!(names.len(), 3);
        assert!(size("single") >= 64_000);
        assert_eq!(
            size("linked"),
            size("single"),
            "a hard link must not count twice"
        );
        assert!(
            entries
                .iter()
                .find(|entry| entry.name == ".cache")
                .unwrap()
                .is_hidden
        );
        assert!(total >= entries.iter().map(|entry| entry.bytes).sum::<u64>() + 8_000);
        assert!(entries
            .windows(2)
            .all(|pair| pair[0].bytes >= pair[1].bytes));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn folder_usage_round_trips_through_the_database() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            crate::app::db::apply_migrations(&pool).await.unwrap();

            let empty = load_folder_usage(&pool, "/Users/me", true).await.unwrap();
            assert_eq!(empty.scanned_at, None);
            assert!(empty.scanning);
            assert!(empty.entries.is_empty());

            let entry = |name: &str, bytes: u64| FolderUsageEntry {
                name: name.into(),
                path: format!("/Users/me/{name}"),
                bytes,
                is_hidden: name.starts_with('.'),
            };
            save_folder_usage(
                &pool,
                "/Users/me",
                900,
                10,
                None,
                &[entry("Work", 500), entry(".cache", 300)],
            )
            .await
            .unwrap();
            save_folder_usage(
                &pool,
                "/Users/me",
                700,
                20,
                None,
                &[entry(".cache", 400), entry("Music", 100)],
            )
            .await
            .unwrap();

            let usage = load_folder_usage(&pool, "/Users/me", false).await.unwrap();
            assert_eq!(usage.total_bytes, 700);
            assert_eq!(usage.scanned_at, Some(20));
            assert_eq!(
                usage.entries,
                vec![entry(".cache", 400), entry("Music", 100)]
            );
        });
    }
}
