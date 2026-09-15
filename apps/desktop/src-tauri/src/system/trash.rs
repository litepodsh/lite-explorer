use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::explorer::paths::home_location;
use crate::system::folder_usage::tree_usage;

/// a copy of deleted files on each mounted volume under `.Trashes/<uid>`.
pub fn trash_paths() -> Vec<PathBuf> {
    let home = home_location().map(|home| home.path);
    let mut paths = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = &home {
            paths.push(PathBuf::from(home).join(".Trash"));
        }
        if let Ok(volumes) = fs::read_dir("/Volumes") {
            let uid = unsafe { libc::getuid() };
            for entry in volumes.filter_map(Result::ok) {
                let trash = entry.path().join(".Trashes").join(uid.to_string());
                if trash.is_dir() {
                    paths.push(trash);
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(home) = &home {
            paths.push(PathBuf::from(home).join(".local/share/Trash"));
        }
    }
    #[cfg(target_os = "windows")]
    {
        for drive in b'A'..=b'Z' {
            let root = format!("{}:\\", drive as char);
            let trash = PathBuf::from(root).join("$Recycle.Bin");
            if trash.is_dir() {
                paths.push(trash);
            }
        }
    }
    paths
}

/// Sizes a single trash location, returning `None` when its root can’t be read (e.g. Full Disk
/// Access not granted). `Some(0)` means the trash is genuinely empty, which is different from
/// "unreadable" — that distinction is what lets the UI tell the user they need to grant access.
pub fn trash_dir_usage(path: &Path, seen: &mut HashSet<(u64, u64)>) -> Option<u64> {
    // The top-level read must succeed; otherwise we can’t measure this trash at all.
    fs::read_dir(path).ok()?;
    Some(tree_usage(path, None, seen, &mut |_| {}))
}

/// Sums the size of every trash location. Returns `None` when no trash location could be measured
/// (none exist, or every one of them is unreadable — usually a Full Disk Access problem), so the
/// caller can tell "can’t measure" apart from "measured and empty".
pub fn trash_bytes() -> Option<u64> {
    let paths = trash_paths();
    if paths.is_empty() {
        return None;
    }
    let mut seen = HashSet::new();
    let mut measured_any = false;
    let mut total: u64 = 0;
    for path in paths.iter().filter(|path| path.exists()) {
        if let Some(bytes) = trash_dir_usage(path, &mut seen) {
            total += bytes;
            measured_any = true;
        }
    }
    measured_any.then_some(total)
}

#[tauri::command]
pub fn empty_trash() -> Result<Option<u64>, String> {
    for path in trash_paths() {
        let Ok(read) = fs::read_dir(&path) else {
            continue;
        };
        for entry in read.filter_map(Result::ok) {
            let item = entry.path();
            let _ = if item.is_dir() {
                fs::remove_dir_all(&item)
            } else {
                fs::remove_file(&item)
            };
        }
    }
    Ok(trash_bytes())
}

/// Sizes the trash on demand, without re-walking the whole home folder. Used to detect whether
/// Full Disk Access is already granted (a `Some` result means the protected locations were read).
#[tauri::command]
pub fn trash_usage() -> Option<u64> {
    trash_bytes()
}

/// Opens System Settings on macOS, landing on the Full Disk Access pane so the user can grant
/// liteexplorer access to the Trash. Other platforms are a no-op for now.
#[tauri::command]
pub fn open_system_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Opens the OS trash so the user can inspect or empty it manually, even when liteexplorer can’t
/// measure its size (e.g. Full Disk Access not granted). macOS opens `~/.Trash` in Finder, Linux
/// opens the XDG trash (falling back to `~/.Trash`), and Windows opens the Recycle Bin.
#[tauri::command]
pub fn open_trash() -> Result<(), String> {
    let home = home_location().map(|home| home.path).unwrap_or_default();
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(format!("{home}/.Trash"))
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let target = format!("{home}/.local/share/Trash");
        let fallback = format!("{home}/.Trash");
        let path = if Path::new(&target).is_dir() {
            target
        } else {
            fallback
        };
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("shell:RecycleBinFolder")
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Touches Full Disk Access-protected trash locations so macOS shows its native permission prompt
/// and adds liteexplorer to the Full Disk Access list. Errors are expected (access denied) and
/// ignored; the side effect is registering the app. Other platforms are a no-op.
#[tauri::command]
pub fn request_full_disk_access() {
    #[cfg(target_os = "macos")]
    {
        let uid = unsafe { libc::getuid() };
        let _ = fs::read_dir("/.Trashes");
        let _ = fs::read_dir(format!("/.Trashes/{uid}"));
        if let Ok(volumes) = fs::read_dir("/Volumes") {
            for volume in volumes.filter_map(Result::ok) {
                let _ = fs::read_dir(volume.path().join(".Trashes").join(uid.to_string()));
            }
        }
    }
}
