use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Emitter};

use crate::system::volumes::device_id;

#[derive(Serialize, Clone)]
pub struct DirectorySizeEntry {
    path: String,
    size: u64,
}

pub const DIRECTORY_SIZE_SCAN_INTERVAL: Duration = Duration::from_millis(120);
const DIRECTORY_SIZE_BATCH: usize = 64;
const DIRECTORY_SIZE_MAX_SCAN_REGISTRY: usize = 8;
// Threads held back from the scan so the UI and OS keep breathing room.
const DIRECTORY_SIZE_WORKER_RESERVE: usize = 2;

// One worker per available core minus the reserve, never more than the work at hand.
fn size_scan_worker_count(children: usize) -> usize {
    let available = std::thread::available_parallelism()
        .map_or(4, |cores| cores.get())
        .saturating_sub(DIRECTORY_SIZE_WORKER_RESERVE)
        .max(1);
    available.min(children)
}

pub struct DirectorySizeScans(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl DirectorySizeScans {
    fn register(&self, request_id: &str) -> Option<Arc<AtomicBool>> {
        let mut scans = self.0.lock().unwrap();
        if scans.len() >= DIRECTORY_SIZE_MAX_SCAN_REGISTRY || scans.contains_key(request_id) {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        scans.insert(request_id.to_string(), Arc::clone(&cancelled));
        Some(cancelled)
    }

    fn unregister(&self, request_id: &str) {
        self.0.lock().unwrap().remove(request_id);
    }

    fn cancel(&self, request_id: &str) -> bool {
        match self.0.lock().unwrap().get(request_id) {
            Some(cancelled) => {
                cancelled.store(true, Ordering::SeqCst);
                true
            }
            None => false,
        }
    }
}

impl Default for DirectorySizeScans {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

fn scan_registry() -> &'static DirectorySizeScans {
    static REGISTRY: OnceLock<DirectorySizeScans> = OnceLock::new();
    REGISTRY.get_or_init(DirectorySizeScans::default)
}

#[derive(Serialize, Clone)]
pub struct DirectorySizeUpdate {
    path: String,
    sizes: Vec<DirectorySizeEntry>,
}

#[derive(Serialize, Clone)]
pub struct MeasuredSize {
    path: String,
    size: u64,
    complete: bool,
}

#[derive(Serialize, Clone)]
pub struct DirectorySizeProgress {
    sizes: Vec<MeasuredSize>,
    done: bool,
    error: Option<String>,
}

// Logical file sizes, matching the Size column. Never follows links or crosses volumes.
fn measure_entry(
    path: &Path,
    device: Option<u64>,
    cancelled: &AtomicBool,
    report: &mut impl FnMut(u64),
) -> (u64, bool) {
    let mut size = 0u64;
    let mut complete = true;
    let mut pending = vec![path.to_path_buf()];
    while let Some(path) = pending.pop() {
        if cancelled.load(Ordering::Relaxed) {
            return (size, false);
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        if metadata.is_dir() {
            if device.is_some() && device_id(&metadata) != device {
                complete = false;
                continue;
            }
            match fs::read_dir(&path) {
                Ok(read) => {
                    for child in read {
                        if cancelled.load(Ordering::Relaxed) {
                            return (size, false);
                        }
                        match child {
                            Ok(child) => pending.push(child.path()),
                            Err(_) => complete = false,
                        }
                    }
                }
                Err(_) => complete = false,
            }
        } else {
            size = size.saturating_add(metadata.len());
        }
        report(size);
    }
    (size, complete)
}

#[tauri::command]
pub fn cancel_directory_size_scan(request_id: String) {
    scan_registry().cancel(&request_id);
}

#[tauri::command]
pub fn scan_directory_sizes(
    path: String,
    request_id: String,
    on_progress: Channel<DirectorySizeProgress>,
) -> Result<(), String> {
    let root = Path::new(&path);
    if !root.is_absolute() {
        return Err("Expected an absolute folder path".into());
    }
    let cancelled = scan_registry()
        .register(&request_id)
        .ok_or("Too many active size scans or duplicate request")?;
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let root = Path::new(&path);
            let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
            if !metadata.is_dir() {
                return Err("Expected a folder, not a file or link".into());
            }
            let device = device_id(&metadata);
            let read = fs::read_dir(root).map_err(|e| e.to_string())?;
            // Drain the folder up front so a listing error still aborts the scan,
            // then measure the children across a small worker pool.
            let mut children: Vec<(PathBuf, String)> = Vec::new();
            for child in read {
                let child = child.map_err(|e| e.to_string())?;
                let child_path = child.path();
                let name = child_path.to_string_lossy().into_owned();
                children.push((child_path, name));
            }
            let workers = size_scan_worker_count(children.len());
            if workers > 0 {
                let (jobs_tx, jobs_rx) = mpsc::channel::<(PathBuf, String)>();
                let jobs_rx = Arc::new(Mutex::new(jobs_rx));
                thread::scope(|scope| {
                    for _ in 0..workers {
                        let jobs_rx = Arc::clone(&jobs_rx);
                        let on_progress = on_progress.clone();
                        let cancelled = Arc::clone(&cancelled);
                        scope.spawn(move || {
                            let mut batch: Vec<MeasuredSize> = Vec::new();
                            let mut last = Instant::now();
                            while let Ok((child_path, name)) = jobs_rx.lock().unwrap().recv() {
                                if cancelled.load(Ordering::Relaxed) {
                                    break;
                                }
                                let (size, complete) =
                                    measure_entry(&child_path, device, &cancelled, &mut |size| {
                                        if last.elapsed() >= DIRECTORY_SIZE_SCAN_INTERVAL {
                                            batch.push(MeasuredSize {
                                                path: name.clone(),
                                                size,
                                                complete: false,
                                            });
                                            if on_progress
                                                .send(DirectorySizeProgress {
                                                    sizes: std::mem::take(&mut batch),
                                                    done: false,
                                                    error: None,
                                                })
                                                .is_err()
                                            {
                                                cancelled.store(true, Ordering::Relaxed);
                                            }
                                            last = Instant::now();
                                        }
                                    });
                                batch.push(MeasuredSize {
                                    path: name,
                                    size,
                                    complete,
                                });
                                if batch.len() >= DIRECTORY_SIZE_BATCH
                                    || last.elapsed() >= DIRECTORY_SIZE_SCAN_INTERVAL
                                {
                                    if on_progress
                                        .send(DirectorySizeProgress {
                                            sizes: std::mem::take(&mut batch),
                                            done: false,
                                            error: None,
                                        })
                                        .is_err()
                                    {
                                        cancelled.store(true, Ordering::Relaxed);
                                        break;
                                    }
                                    last = Instant::now();
                                }
                            }
                            if !batch.is_empty() {
                                let _ = on_progress.send(DirectorySizeProgress {
                                    sizes: std::mem::take(&mut batch),
                                    done: false,
                                    error: None,
                                });
                            }
                        });
                    }
                    for child in children {
                        if cancelled.load(Ordering::Relaxed) || jobs_tx.send(child).is_err() {
                            break;
                        }
                    }
                    drop(jobs_tx);
                });
            }
            Ok(())
        })();
        scan_registry().unregister(&request_id);
        let _ = on_progress.send(DirectorySizeProgress {
            sizes: vec![],
            done: true,
            error: result.err(),
        });
    });
    Ok(())
}

#[tauri::command]
pub fn compute_directory_sizes(path: String, app: AppHandle) {
    std::thread::spawn(move || {
        let folder = Path::new(&path);
        let mut batch: Vec<DirectorySizeEntry> = Vec::with_capacity(64);
        let flush = |batch: &mut Vec<DirectorySizeEntry>, app: &AppHandle, path: &str| {
            if batch.is_empty() {
                return;
            }
            let payload = DirectorySizeUpdate {
                path: path.to_string(),
                sizes: std::mem::take(batch),
            };
            let _ = app.emit("directory-sizes", payload);
        };
        if let Ok(read) = fs::read_dir(folder) {
            for entry in read.filter_map(Result::ok) {
                let child_path = entry.path().to_string_lossy().into_owned();
                let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                    continue;
                };
                if metadata.is_dir() {
                    continue;
                }
                let size = metadata.len();
                batch.push(DirectorySizeEntry {
                    path: child_path,
                    size,
                });
                if batch.len() >= 64 {
                    flush(&mut batch, &app, &path);
                }
            }
        }
        flush(&mut batch, &app, &path);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_reserves_threads_and_caps_at_work() {
        assert_eq!(size_scan_worker_count(0), 0);
        assert_eq!(size_scan_worker_count(1), 1);
        assert!(size_scan_worker_count(usize::MAX) >= 1);
        assert!(size_scan_worker_count(2) <= 2);
    }

    #[test]
    fn measures_nested_sizes_and_handles_cancellation_and_missing_paths() {
        let root = std::env::temp_dir().join(format!("directory-sizes-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("nested/empty")).unwrap();
        fs::write(root.join("one"), [0; 3]).unwrap();
        fs::write(root.join("nested/two"), [0; 7]).unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(measure_entry(&root, None, &cancel, &mut |_| {}), (10, true));
        assert_eq!(
            measure_entry(&root.join("nested/empty"), None, &cancel, &mut |_| {}),
            (0, true)
        );
        assert_eq!(
            measure_entry(&root.join("missing"), None, &cancel, &mut |_| {}),
            (0, false)
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
            let link_size = fs::symlink_metadata(root.join("loop")).unwrap().len();
            assert_eq!(
                measure_entry(&root, None, &cancel, &mut |_| {}),
                (10 + link_size, true)
            );
        }
        let (_, complete) = measure_entry(&root, None, &cancel, &mut |_| {
            cancel.store(true, Ordering::Relaxed);
        });
        assert!(!complete);
        fs::remove_dir_all(root).unwrap();
        let registry = DirectorySizeScans::default();
        let flag = registry.register("first").unwrap();
        assert!(registry.register("first").is_none());
        assert!(registry.cancel("first"));
        assert!(flag.load(Ordering::Relaxed));
        registry.unregister("first");
        assert!(!registry.cancel("first"));
    }
}
