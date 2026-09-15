use std::{fs, path::Path};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone)]
pub struct DirectorySizeEntry {
    path: String,
    size: u64,
}

#[derive(Serialize, Clone)]
pub struct DirectorySizeUpdate {
    path: String,
    sizes: Vec<DirectorySizeEntry>,
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
                let size = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
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
