use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use tauri::{
    menu::{
        AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
    },
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow, WindowEvent,
};
#[cfg(target_os = "macos")]
use trash::macos::{DeleteMethod, TrashContextExtMacos};

mod archive;
mod network;
mod remote;
mod transfer;

struct Database(SqlitePool);

const WINDOW_STATE_FILE: &str = "window-state.json";

#[derive(Deserialize, Serialize)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn save_window_state(window: &WebviewWindow) {
    let (Ok(position), Ok(size), Ok(path)) = (
        window.outer_position(),
        window.outer_size(),
        window.app_handle().path().app_config_dir(),
    ) else {
        return;
    };
    let _ = fs::create_dir_all(&path);
    let _ = fs::write(
        path.join(WINDOW_STATE_FILE),
        serde_json::to_vec(&WindowState {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
        .unwrap_or_default(),
    );
}

fn restore_window_state(window: &WebviewWindow) {
    let Ok(path) = window.app_handle().path().app_config_dir() else {
        return;
    };
    let Some(state) = fs::read(path.join(WINDOW_STATE_FILE))
        .ok()
        .and_then(|saved| serde_json::from_slice::<WindowState>(&saved).ok())
    else {
        return;
    };
    let monitor = window
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|monitor| {
                let area = monitor.work_area();
                state.x < area.position.x + area.size.width as i32
                    && state.x + state.width as i32 > area.position.x
                    && state.y < area.position.y + area.size.height as i32
                    && state.y + state.height as i32 > area.position.y
            })
        })
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let area = monitor.work_area();
    let (position, size) = clamp_window_state(&state, area.position, area.size);
    let _ = window.set_size(size);
    let _ = window.set_position(position);
}

fn clamp_window_state(
    state: &WindowState,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let width = state.width.min(size.width);
    let height = state.height.min(size.height);
    let x = state.x.clamp(
        position.x,
        position.x + size.width.saturating_sub(width) as i32,
    );
    let y = state.y.clamp(
        position.y,
        position.y + size.height.saturating_sub(height) as i32,
    );
    (
        PhysicalPosition::new(x, y),
        PhysicalSize::new(width, height),
    )
}

#[derive(Serialize)]
struct Location {
    name: String,
    path: String,
    kind: String,
}

#[derive(Serialize)]
struct DirectoryEntry {
    name: String,
    path: String,
    is_directory: bool,
    is_hidden: bool,
    size: Option<u64>,
    created: Option<u64>,
    /// Set for entries that aren't plain files or folders, like `"bucket"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
}

const PREVIEW_SNIFF_BYTES: usize = 8 * 1024;
const PREVIEW_MAX_BYTES: usize = 2 * 1024 * 1024;
const IMAGE_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
enum PreviewKind {
    Text,
    Binary,
    Directory,
    Image,
}

#[derive(Serialize, Debug)]
struct FilePreview {
    name: String,
    size: u64,
    created: Option<u64>,
    modified: Option<u64>,
    kind: PreviewKind,
    content: Option<String>,
    src: Option<String>,
    truncated: bool,
}

#[derive(Serialize)]
struct Recent {
    name: String,
    path: String,
    kind: String,
    opened_at: i64,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

const THIRTY_DAYS_SECS: i64 = 30 * 24 * 3600;

fn home_location() -> Option<Location> {
    let path = std::env::var_os(if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    })?;
    let path = PathBuf::from(path);
    Some(Location {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: "home".into(),
    })
}

fn startup_volume_name() -> String {
    #[cfg(target_os = "macos")]
    {
        return Command::new("diskutil")
            .args(["info", "/"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|output| {
                output.lines().find_map(|line| {
                    line.trim()
                        .strip_prefix("Volume Name:")
                        .map(|name| name.trim().to_owned())
                })
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Macintosh HD".into());
    }

    #[cfg(target_os = "windows")]
    {
        "Local Disk (C:)".into()
    }

    #[cfg(target_os = "linux")]
    {
        "File System".into()
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        "System Drive".into()
    }
}

fn system_locations() -> Vec<Location> {
    let root = if cfg!(target_os = "windows") {
        "C:\\"
    } else {
        "/"
    };
    let mut locations = vec![Location {
        name: startup_volume_name(),
        path: root.into(),
        kind: "volume".into(),
    }];
    if let Some(home) = home_location() {
        locations.push(home);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        locations.extend(macos_cloud_locations(&home));
    }
    locations
}

/// Cloud providers installed by macOS expose ordinary directories. Keeping them as local
/// locations lets their own File Provider handle syncing and on-demand downloads.
#[cfg(target_os = "macos")]
fn macos_cloud_locations(home: &Path) -> Vec<Location> {
    let mut locations = Vec::new();
    let icloud = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    if icloud.is_dir() {
        locations.push(Location {
            name: "iCloud Drive".into(),
            path: icloud.to_string_lossy().into_owned(),
            kind: "cloud".into(),
        });
    }

    let cloud_storage = home.join("Library/CloudStorage");
    let google_drives: Vec<_> = fs::read_dir(cloud_storage)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("GoogleDrive-")
        })
        .collect();
    let multiple_accounts = google_drives.len() > 1;
    for drive in google_drives {
        let suffix = drive
            .file_name()
            .to_string_lossy()
            .trim_start_matches("GoogleDrive-")
            .to_string();
        locations.push(Location {
            name: if multiple_accounts && !suffix.is_empty() {
                format!("Google Drive ({suffix})")
            } else {
                "Google Drive".into()
            },
            path: drive.path().to_string_lossy().into_owned(),
            kind: "cloud".into(),
        });
    }
    locations
}

fn location(path: PathBuf) -> Option<Location> {
    Some(Location {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: "folder".into(),
    })
}

fn add_location(locations: &mut Vec<Location>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if path.is_dir() && seen.insert(path.clone()) {
        if let Some(location) = location(path) {
            locations.push(location);
        }
    }
}

fn file_url_path(value: &str) -> Option<PathBuf> {
    let value = value.strip_prefix("file://")?;
    let value = value.strip_prefix("localhost/").unwrap_or(value);
    let value = value.strip_prefix('/').unwrap_or(value);
    let mut bytes = Vec::with_capacity(value.len());
    let mut chars = value.bytes();
    while let Some(byte) = chars.next() {
        if byte == b'%' {
            let high = chars.next()?.to_ascii_lowercase();
            let low = chars.next()?.to_ascii_lowercase();
            let hex = |byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                _ => None,
            };
            bytes.push(hex(high)? * 16 + hex(low)?);
        } else {
            bytes.push(byte);
        }
    }
    Some(PathBuf::from(format!(
        "/{}",
        String::from_utf8_lossy(&bytes)
    )))
}

fn standard_favorites() -> Vec<Location> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    for name in [
        "Desktop",
        "Documents",
        "Downloads",
        "Movies",
        "Music",
        "Pictures",
    ] {
        add_location(&mut locations, &mut seen, home.join(name));
    }
    locations
}

#[cfg(target_os = "macos")]
fn macos_favorites() -> Vec<Location> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return standard_favorites();
    };
    let list = home.join("Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.FavoriteItems.sfl2");
    let output = Command::new("/usr/bin/sfltool")
        .arg("list")
        .arg(&list)
        .output()
        .ok();
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    if let Some(output) = output {
        for value in String::from_utf8_lossy(&output.stdout).split_whitespace() {
            if let Some(path) = value.find("file://").and_then(|start| {
                file_url_path(value[start..].trim_matches(|character: char| {
                    matches!(character, '\"' | ',' | ')' | ']' | '>' | '<')
                }))
            }) {
                add_location(&mut locations, &mut seen, path);
            }
        }
    }
    if locations.is_empty() {
        standard_favorites()
    } else {
        locations
    }
}

#[cfg(target_os = "linux")]
fn linux_favorites() -> Vec<Location> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    for bookmarks in [
        config.join("gtk-4.0/bookmarks"),
        config.join("gtk-3.0/bookmarks"),
    ] {
        if let Ok(contents) = fs::read_to_string(bookmarks) {
            for line in contents.lines() {
                if let Some(path) = line.split_whitespace().next().and_then(file_url_path) {
                    add_location(&mut locations, &mut seen, path);
                }
            }
        }
    }
    for name in [
        "Desktop",
        "Documents",
        "Downloads",
        "Music",
        "Pictures",
        "Videos",
    ] {
        add_location(&mut locations, &mut seen, home.join(name));
    }
    locations
}

#[tauri::command]
fn favorites() -> Vec<Location> {
    #[cfg(target_os = "macos")]
    {
        return macos_favorites();
    }
    #[cfg(target_os = "linux")]
    {
        return linux_favorites();
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        standard_favorites()
    }
}

#[cfg(target_os = "macos")]
fn coordinated_read<T>(
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
fn coordinated_read<T>(_: &Path, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    action()
}

#[cfg(target_os = "macos")]
fn coordinated_write<T>(
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
fn coordinated_write<T>(_: &Path, action: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    action()
}

fn directory_entries(path: &Path) -> Result<Vec<DirectoryEntry>, String> {
    if !path.is_dir() {
        return Err(format!("{} is not a directory", path.display()));
    }
    let entries: Vec<DirectoryEntry> = fs::read_dir(path)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let is_directory = entry.file_type().ok()?.is_dir();
            let name = entry.file_name().to_string_lossy().into_owned();
            let metadata = fs::metadata(entry.path()).ok()?;
            Some(DirectoryEntry {
                is_hidden: name.starts_with('.'),
                name,
                path: entry.path().to_string_lossy().into_owned(),
                is_directory,
                size: None,
                created: epoch_millis(metadata.created()),
                kind: None,
            })
        })
        .collect();
    Ok(entries)
}

#[tauri::command]
async fn read_directory(
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
        let path = PathBuf::from(path);
        coordinated_read(&path, || directory_entries(&path))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn single_entry(path: &Path) -> Result<DirectoryEntry, String> {
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
        kind: None,
    })
}

fn unique_name(parent: &Path, base: &str) -> String {
    if !parent.join(base).exists() {
        return base.to_string();
    }
    let mut index = 2;
    loop {
        let numbered = format!("{} {}", base, index);
        if !parent.join(&numbered).exists() {
            return numbered;
        }
        index += 1;
    }
}

#[tauri::command]
async fn create_item(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    parent: String,
    kind: String,
    name: String,
) -> Result<DirectoryEntry, String> {
    if network::servers::is_server_path(&parent) {
        return network::servers::create_item(&database.0, &sessions, &parent, &kind, &name).await;
    }
    if remote::is_remote_path(&parent) {
        return remote::write::create_item(&database.0, &clients, &parent, &kind, &name).await;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let parent_path = PathBuf::from(&parent);
        coordinated_write(&parent_path, || create_local_item(parent, kind, name))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn create_local_item(parent: String, kind: String, name: String) -> Result<DirectoryEntry, String> {
    let parent_path = Path::new(&parent);
    if !parent_path.is_dir() {
        return Err(format!("{} is not a directory", parent_path.display()));
    }
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("name must not be empty".into());
    }
    let final_name = unique_name(parent_path, trimmed);
    let target = parent_path.join(&final_name);
    if kind == "folder" {
        fs::create_dir(&target).map_err(|error| error.to_string())?;
    } else {
        fs::File::create(&target).map_err(|error| error.to_string())?;
    }
    single_entry(&target)
}

#[tauri::command]
async fn rename_item(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
    new_name: String,
) -> Result<DirectoryEntry, String> {
    if network::servers::is_server_path(&path) {
        return network::servers::rename_item(&database.0, &sessions, &path, &new_name).await;
    }
    if remote::is_remote_path(&path) {
        return remote::write::rename_item(&database.0, &clients, &path, &new_name).await;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        coordinated_write(&source, || rename_local_item(path, new_name))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn rename_local_item(path: String, new_name: String) -> Result<DirectoryEntry, String> {
    let source = Path::new(&path);
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("name must not be empty".into());
    }
    let parent = source.parent().ok_or("invalid path")?;
    let target = parent.join(trimmed);
    fs::rename(source, &target).map_err(|error| error.to_string())?;
    single_entry(&target)
}

#[derive(Serialize)]
struct AppInfo {
    name: String,
    path: String,
    bundle_id: String,
    icon: Option<String>,
}

#[tauri::command]
fn open_with_apps(path: String) -> Result<Vec<AppInfo>, String> {
    #[cfg(target_os = "macos")]
    {
        use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
        use core_foundation_sys::base::{CFRelease, CFTypeRef};
        use core_foundation_sys::string::{
            kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString,
            CFStringGetLength, CFStringRef,
        };
        use core_foundation_sys::url::{kCFURLPOSIXPathStyle, CFURLCopyFileSystemPath, CFURLRef};
        use std::ffi::{CStr, CString};
        use std::path::Path;
        use std::ptr;

        #[link(name = "CoreServices", kind = "framework")]
        extern "C" {
            fn LSCopyApplicationURLsForURL(inURL: CFURLRef, roleMask: u32) -> CFArrayRef;
            fn LSCopyDisplayNameForURL(inURL: CFURLRef, outDisplayName: *mut CFStringRef) -> u8;
        }

        const K_LS_ROLES_ALL: u32 = 0xFFFF_FFFF;

        unsafe fn cfurl_from_path(path: &str) -> Option<CFURLRef> {
            let cstr = CString::new(path).ok()?;
            let cfstr: CFStringRef =
                CFStringCreateWithCString(ptr::null(), cstr.as_ptr(), kCFStringEncodingUTF8);
            if cfstr.is_null() {
                return None;
            }
            let url = core_foundation_sys::url::CFURLCreateWithFileSystemPath(
                ptr::null(),
                cfstr,
                kCFURLPOSIXPathStyle,
                0,
            );
            CFRelease(cfstr as CFTypeRef);
            if url.is_null() {
                None
            } else {
                Some(url)
            }
        }

        unsafe fn cfstring_to_string(string: CFStringRef) -> String {
            if string.is_null() {
                return String::new();
            }
            let length = CFStringGetLength(string);
            let mut buffer = vec![0i8; (length as usize) * 4 + 1];
            let ok = CFStringGetCString(
                string,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                kCFStringEncodingUTF8,
            );
            if ok == 0 {
                return String::new();
            }
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }

        unsafe {
            let url = match cfurl_from_path(&path) {
                Some(url) => url,
                None => return Ok(vec![]),
            };
            let apps = LSCopyApplicationURLsForURL(url, K_LS_ROLES_ALL);
            CFRelease(url as CFTypeRef);
            if apps.is_null() {
                return Ok(vec![]);
            }
            let count = CFArrayGetCount(apps);
            let mut result = Vec::with_capacity(count as usize);
            for index in 0..count {
                let app_url = CFArrayGetValueAtIndex(apps, index) as CFURLRef;
                if app_url.is_null() {
                    continue;
                }
                let fs_path = CFURLCopyFileSystemPath(app_url, kCFURLPOSIXPathStyle);
                let app_path = cfstring_to_string(fs_path);
                CFRelease(fs_path as CFTypeRef);
                let mut display: CFStringRef = ptr::null_mut();
                LSCopyDisplayNameForURL(app_url, &mut display);
                let name = if display.is_null() {
                    Path::new(&app_path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    let string = cfstring_to_string(display);
                    CFRelease(display as CFTypeRef);
                    string
                };
                result.push(AppInfo {
                    name,
                    path: app_path,
                    bundle_id: String::new(),
                    icon: None,
                });
            }
            CFRelease(apps as CFTypeRef);
            // De-duplicate by path (LaunchServices can return duplicates).
            let mut seen = std::collections::HashSet::new();
            result.retain(|app| seen.insert(app.path.clone()));
            for app in &mut result {
                app.icon = app_icon_data_url(&app.path);
            }
            Ok(result)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Ok(vec![])
    }
}

/// The application the OS uses to open `path` by default (what a double-click in Finder launches).
#[tauri::command]
fn default_app(path: String) -> Option<AppInfo> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{class, msg_send, rc::autoreleasepool, runtime::AnyObject};
        use std::ffi::{c_char, CStr, CString};

        let c_path = CString::new(path).ok()?;
        let app_path = autoreleasepool(|_| unsafe {
            let string: *mut AnyObject =
                msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
            let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: string];
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app_url: *mut AnyObject = msg_send![workspace, URLForApplicationToOpenURL: url];
            if app_url.is_null() {
                return None;
            }
            let app_path: *mut AnyObject = msg_send![app_url, path];
            let utf8: *const c_char = msg_send![app_path, UTF8String];
            (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
        })?;
        let name = Path::new(&app_path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| app_path.clone());
        let icon = app_icon_data_url(&app_path);
        Some(AppInfo {
            name,
            path: app_path,
            bundle_id: String::new(),
            icon,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// PNG data URL of the Finder icon for `path` (an app bundle), cached per path.
#[cfg(target_os = "macos")]
fn app_icon_data_url(path: &str) -> Option<String> {
    use objc2::{
        class,
        encode::{Encode, Encoding, RefEncode},
        msg_send,
        rc::autoreleasepool,
        runtime::AnyObject,
    };
    use std::{
        collections::HashMap,
        ffi::{c_void, CString},
        sync::{Mutex, OnceLock},
    };

    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }
    unsafe impl RefEncode for CGRect {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }

    #[repr(C)]
    struct CGImage {
        _private: [u8; 0],
    }
    unsafe impl RefEncode for CGImage {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGImage", &[]));
    }

    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(icon) = cache.lock().ok()?.get(path) {
        return icon.clone();
    }

    let c_path = CString::new(path).ok()?;
    let icon = autoreleasepool(|_| unsafe {
        let string: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let image: *mut AnyObject = msg_send![workspace, iconForFile: string];
        if image.is_null() {
            return None;
        }
        // 32pt at 2x keeps the 16px menu icon sharp on Retina displays.
        let _: () = msg_send![image, setSize: CGSize { width: 32.0, height: 32.0 }];
        let null_object: *mut AnyObject = std::ptr::null_mut();
        let cg_image: *mut CGImage = msg_send![
            image,
            CGImageForProposedRect: std::ptr::null_mut::<CGRect>(),
            context: null_object,
            hints: null_object
        ];
        if cg_image.is_null() {
            return None;
        }
        let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut AnyObject = msg_send![rep, initWithCGImage: cg_image];
        if rep.is_null() {
            return None;
        }
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        // NSBitmapImageFileTypePNG
        let data: *mut AnyObject =
            msg_send![rep, representationUsingType: 4usize, properties: properties];
        let encoded = if data.is_null() {
            None
        } else {
            let bytes: *const c_void = msg_send![data, bytes];
            let length: usize = msg_send![data, length];
            let slice = std::slice::from_raw_parts(bytes as *const u8, length);
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(slice)
            ))
        };
        let _: () = msg_send![rep, release];
        encoded
    });

    if let Ok(mut cache) = cache.lock() {
        cache.insert(path.to_owned(), icon.clone());
    }
    icon
}

#[tauri::command]
fn open_with(path: String, app_path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", &app_path, &path])
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("failed to open with {app_path} (exit {status})"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, app_path);
        Err("Open With is only supported on macOS".into())
    }
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");

    let status = command
        .arg(&path)
        .status()
        .map_err(|error| error.to_string())?;
    // explorer.exe returns exit code 1 even when it succeeds.
    if status.success() || cfg!(target_os = "windows") {
        Ok(())
    } else {
        Err(format!("failed to open {path} (exit {status})"))
    }
}

/// Reveals `path` in the system file manager, selecting it when the OS supports it.
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg("-R").arg(&path).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("explorer")
        .arg(format!("/select,{path}"))
        .status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = {
        let parent = Path::new(&path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        Command::new("xdg-open").arg(parent).status()
    };

    status
        .map_err(|error| error.to_string())
        .and_then(|status| {
            // explorer.exe returns exit code 1 even when it succeeds.
            if status.success() || cfg!(target_os = "windows") {
                Ok(())
            } else {
                Err(format!("failed to reveal {path} (exit {status})"))
            }
        })
}

#[derive(Serialize, Clone)]
struct DirectorySizeEntry {
    path: String,
    size: u64,
}

#[derive(Serialize, Clone)]
struct DirectorySizeUpdate {
    path: String,
    sizes: Vec<DirectorySizeEntry>,
}

#[tauri::command]
fn compute_directory_sizes(path: String, app: AppHandle) {
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

fn epoch_millis(time: std::io::Result<SystemTime>) -> Option<u64> {
    time.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

fn utf8_boundary(bytes: &[u8], limit: usize) -> usize {
    let mut end = limit.min(bytes.len());
    while end > 0 && end < bytes.len() && (bytes[end] & 0b1100_0000) == 0b1000_0000 {
        end -= 1;
    }
    end
}

fn image_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn file_preview(path: &Path) -> Result<FilePreview, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let mut preview = FilePreview {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        size: metadata.len(),
        created: epoch_millis(metadata.created()),
        modified: epoch_millis(metadata.modified()),
        kind: PreviewKind::Directory,
        content: None,
        src: None,
        truncated: false,
    };
    if metadata.is_dir() {
        return Ok(preview);
    }

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        if let Some(mime) = image_mime(extension) {
            let file = fs::File::open(path).map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            file.take(IMAGE_MAX_BYTES as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() >= IMAGE_MAX_BYTES {
                return Err(format!(
                    "Image exceeds {} MB preview limit",
                    IMAGE_MAX_BYTES / (1024 * 1024)
                ));
            }
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            preview.kind = PreviewKind::Image;
            preview.src = Some(format!("data:{};base64,{}", mime, encoded));
            return Ok(preview);
        }
    }

    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = file.take(PREVIEW_MAX_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    (&mut reader)
        .take(PREVIEW_SNIFF_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.contains(&0) {
        preview.kind = PreviewKind::Binary;
        return Ok(preview);
    }

    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > PREVIEW_MAX_BYTES {
        bytes.truncate(utf8_boundary(&bytes, PREVIEW_MAX_BYTES));
        preview.truncated = true;
    }
    preview.kind = PreviewKind::Text;
    preview.content = Some(String::from_utf8_lossy(&bytes).into_owned());
    Ok(preview)
}

#[tauri::command]
async fn read_file_preview(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<FilePreview, String> {
    if network::servers::is_server_path(&path) {
        return network::servers::file_preview(&database.0, &sessions, &path).await;
    }
    if remote::is_remote_path(&path) {
        return remote::file_preview(&database.0, &clients, &path).await;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        coordinated_read(&path, || file_preview(&path))
    })
    .await
    .map_err(|error| error.to_string())?
}

async fn open_database(app: &tauri::AppHandle) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let data_dir = app.path().app_data_dir()?;
    fs::create_dir_all(&data_dir)?;
    let options = SqliteConnectOptions::new()
        .filename(data_dir.join(".lite_store.sqlite"))
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    apply_migrations(&pool).await?;
    // Installed File Provider accounts may disappear; don't leave dead cloud shortcuts behind.
    sqlx::query("DELETE FROM locations WHERE kind = 'cloud'")
        .execute(&pool)
        .await?;
    for (position, location) in system_locations().iter().enumerate() {
        sqlx::query("INSERT INTO locations (path, name, kind, position) VALUES (?, ?, ?, ?) ON CONFLICT(path) DO UPDATE SET name = excluded.name, kind = excluded.kind, position = excluded.position")
            .bind(&location.path).bind(&location.name).bind(&location.kind).bind(position as i64)
            .execute(&pool).await?;
    }
    Ok(pool)
}

async fn apply_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    let applied =
        sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations WHERE success = 1")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    let migrator = sqlx::migrate!("./migrations");
    for migration in migrator
        .iter()
        .filter(|migration| !applied.contains(&migration.version))
    {
        println!(
            "[lite_store] applying migration {}: {}",
            migration.version, migration.description
        );
    }
    migrator.run(pool).await
}

#[tauri::command]
async fn locations(database: State<'_, Database>) -> Result<Vec<Location>, String> {
    let mut locations: Vec<Location> =
        sqlx::query("SELECT name, path, kind FROM locations ORDER BY position, name")
            .fetch_all(&database.0)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| Location {
                        name: row.get("name"),
                        path: row.get("path"),
                        kind: row.get("kind"),
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())?;
    locations.extend(
        remote::remote_locations(&database.0)
            .await
            .map_err(|error| error.to_string())?,
    );
    locations.extend(
        network::network_locations(&database.0)
            .await
            .map_err(|error| error.to_string())?,
    );
    Ok(locations)
}

fn recent_kind(is_directory: bool) -> String {
    if is_directory {
        "folder".into()
    } else {
        "file".into()
    }
}

async fn insert_recent(
    database: &Database,
    path: &str,
    name: &str,
    kind: &str,
) -> Result<(), String> {
    let opened_at = now_secs();
    sqlx::query(
        "INSERT INTO recents (path, name, kind, opened_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(path) DO UPDATE SET name = excluded.name, kind = excluded.kind, opened_at = excluded.opened_at",
    )
    .bind(path)
    .bind(name)
    .bind(kind)
    .bind(opened_at)
    .execute(&database.0)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM recents WHERE opened_at < ?")
        .bind(opened_at - THIRTY_DAYS_SECS)
        .execute(&database.0)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn record_recent(
    path: String,
    name: String,
    kind: String,
    database: State<'_, Database>,
) -> Result<(), String> {
    insert_recent(&database, &path, &name, &kind).await
}

/// Copies `path` (file or directory tree) into `destination`, choosing a unique
/// name on collision. Records the result in recents and returns its entry.
#[tauri::command]
async fn copy_item(
    path: String,
    destination: String,
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
) -> Result<DirectoryEntry, String> {
    if network::servers::is_server_path(&path) || network::servers::is_server_path(&destination) {
        return network::servers::copy_item(&database.0, &sessions, &path, &destination).await;
    }
    if remote::is_remote_path(&path) || remote::is_remote_path(&destination) {
        return remote::write::copy_item(&database.0, &clients, &path, &destination).await;
    }
    copy_item_inner(&database, &path, &destination).await
}

async fn copy_item_inner(
    database: &Database,
    path: &str,
    destination: &str,
) -> Result<DirectoryEntry, String> {
    let source = Path::new(path);
    let dest_dir = Path::new(destination);
    if !dest_dir.is_dir() {
        return Err(format!("{} is not a directory", dest_dir.display()));
    }
    let base = source
        .file_name()
        .ok_or("invalid source path")?
        .to_string_lossy()
        .into_owned();
    let target = dest_dir.join(unique_name(dest_dir, &base));
    let entry = coordinated_read(source, || {
        coordinated_write(&target, || {
            if source.is_dir() {
                copy_dir_recursive(source, &target)?;
            } else {
                fs::copy(source, &target).map_err(|error| error.to_string())?;
            }
            single_entry(&target)
        })
    })?;
    insert_recent(
        database,
        &entry.path,
        &entry.name,
        &recent_kind(entry.is_directory),
    )
    .await?;
    Ok(entry)
}

/// Moves `path` into `destination`, choosing a unique name on collision.
/// Falls back to copy + delete when rename crosses volumes. Records the result
/// in recents and returns its entry.
#[tauri::command]
async fn move_item(
    path: String,
    destination: String,
    database: State<'_, Database>,
    sessions: State<'_, network::servers::Sessions>,
) -> Result<DirectoryEntry, String> {
    if network::servers::is_server_path(&path) && network::servers::is_server_path(&destination) {
        return network::servers::move_item(&database.0, &sessions, &path, &destination).await;
    }
    if network::servers::is_server_path(&path) || network::servers::is_server_path(&destination) {
        return Err(
            "Moving between a server and this computer isn’t supported. Copy instead.".into(),
        );
    }
    if remote::is_remote_path(&path) || remote::is_remote_path(&destination) {
        return Err("Moving remote items isn’t supported yet".into());
    }
    move_item_inner(&database, &path, &destination).await
}

async fn move_item_inner(
    database: &Database,
    path: &str,
    destination: &str,
) -> Result<DirectoryEntry, String> {
    let source = Path::new(path);
    let dest_dir = Path::new(destination);
    if !dest_dir.is_dir() {
        return Err(format!("{} is not a directory", dest_dir.display()));
    }
    let base = source
        .file_name()
        .ok_or("invalid source path")?
        .to_string_lossy()
        .into_owned();
    let target = dest_dir.join(unique_name(dest_dir, &base));
    let entry = coordinated_write(source, || {
        coordinated_write(&target, || {
            if fs::rename(source, &target).is_err() {
                if source.is_dir() {
                    copy_dir_recursive(source, &target)?;
                } else {
                    fs::copy(source, &target).map_err(|error| error.to_string())?;
                }
                fs::remove_dir_all(source)
                    .or_else(|_| fs::remove_file(source))
                    .map_err(|error| error.to_string())?;
            }
            single_entry(&target)
        })
    })?;
    insert_recent(
        database,
        &entry.path,
        &entry.name,
        &recent_kind(entry.is_directory),
    )
    .await?;
    Ok(entry)
}

#[tauri::command]
async fn trash_item(path: String) -> Result<(), String> {
    eprintln!("[lite_delete] trash {path}");
    let label = path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        coordinated_write(&path, || trash_local_item(&path))
    })
    .await
    .map_err(|error| error.to_string())?;
    match &result {
        Ok(()) => eprintln!("[lite_delete] trashed ok {label}"),
        Err(error) => eprintln!("[lite_delete] failed trash {label}: {error}"),
    }
    result
}

fn trash_local_item(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut context = trash::TrashContext::default();
        context.set_delete_method(DeleteMethod::NsFileManager);
        context.delete(path).map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        trash::delete(path).map_err(|error| error.to_string())
    }
}

#[tauri::command]
async fn delete_item(path: String) -> Result<(), String> {
    eprintln!("[lite_delete] delete {path}");
    let label = path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        coordinated_write(&path, || delete_item_path(&path))
    })
    .await
    .map_err(|error| error.to_string())?;
    match &result {
        Ok(()) => eprintln!("[lite_delete] deleted ok {label}"),
        Err(error) => eprintln!("[lite_delete] failed delete {label}: {error}"),
    }
    result
}

fn delete_item_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|error| error.to_string())
}

fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| error.to_string())?;
    for child in fs::read_dir(from).map_err(|error| error.to_string())? {
        let child = child.map_err(|error| error.to_string())?;
        let child_path = child.path();
        let dest = to.join(child.file_name());
        if child_path.is_dir() {
            copy_dir_recursive(&child_path, &dest)?;
        } else {
            fs::copy(&child_path, &dest).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn recents(database: State<'_, Database>) -> Result<Vec<Recent>, String> {
    let cutoff = now_secs() - THIRTY_DAYS_SECS;
    sqlx::query("SELECT name, path, kind, opened_at FROM recents WHERE opened_at >= ? ORDER BY opened_at DESC")
        .bind(cutoff)
        .fetch_all(&database.0)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| Recent {
                    name: row.get("name"),
                    path: row.get("path"),
                    kind: row.get("kind"),
                    opened_at: row.get("opened_at"),
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn clear_recents(database: State<'_, Database>) -> Result<(), String> {
    sqlx::query("DELETE FROM recents")
        .execute(&database.0)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct DeviceInfo {
    name: String,
    chip: Option<String>,
    cores: usize,
    memory_bytes: Option<u64>,
}

#[derive(Serialize)]
struct VolumeInfo {
    name: String,
    mount_point: String,
    file_system: Option<String>,
    total_bytes: u64,
    free_bytes: u64,
    is_primary: bool,
}

#[derive(Serialize)]
struct DiskOverview {
    device: DeviceInfo,
    volumes: Vec<VolumeInfo>,
    home: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct FolderUsageEntry {
    name: String,
    path: String,
    bytes: u64,
    is_hidden: bool,
}

#[derive(Serialize, Clone, Debug)]
struct FolderUsage {
    root: String,
    total_bytes: u64,
    scanned_at: Option<i64>,
    scanning: bool,
    entries: Vec<FolderUsageEntry>,
    /// Total size of the OS trash, summed across the locations for this platform.
    trash_bytes: Option<u64>,
}

#[derive(Serialize, Clone)]
struct FolderUsageProgress {
    root: String,
    scanned_bytes: u64,
    current: Option<String>,
    entry: Option<FolderUsageEntry>,
}

#[derive(Serialize, Clone)]
struct FolderUsageError {
    root: String,
    message: String,
}

/// Roots with a folder usage scan in flight, so entering the overview twice never starts two walks.
struct FolderScans(std::sync::Mutex<HashSet<String>>);

struct VolumeStats {
    total_bytes: u64,
    free_bytes: u64,
    file_system: Option<String>,
    mounted_on: Option<String>,
}

#[cfg(target_os = "macos")]
fn volume_stats(path: &Path) -> Option<VolumeStats> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block_size = stats.f_bsize as u64;
    let file_system = unsafe { CStr::from_ptr(stats.f_fstypename.as_ptr()) };
    let mounted_on = unsafe { CStr::from_ptr(stats.f_mntonname.as_ptr()) };
    Some(VolumeStats {
        total_bytes: stats.f_blocks as u64 * block_size,
        free_bytes: stats.f_bavail as u64 * block_size,
        file_system: Some(file_system.to_string_lossy().to_uppercase()),
        mounted_on: Some(mounted_on.to_string_lossy().into_owned()),
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn volume_stats(path: &Path) -> Option<VolumeStats> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block_size = stats.f_frsize as u64;
    Some(VolumeStats {
        total_bytes: stats.f_blocks as u64 * block_size,
        free_bytes: stats.f_bavail as u64 * block_size,
        file_system: None,
        mounted_on: None,
    })
}

#[cfg(not(unix))]
fn volume_stats(_path: &Path) -> Option<VolumeStats> {
    None
}

#[cfg(target_os = "macos")]
fn volumes() -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    if let Some(stats) = volume_stats(Path::new("/")) {
        volumes.push(VolumeInfo {
            name: startup_volume_name(),
            mount_point: "/".into(),
            file_system: stats.file_system,
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary: true,
        });
    }
    let Ok(read) = fs::read_dir("/Volumes") else {
        return volumes;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        let mount_point = path.to_string_lossy().into_owned();
        let Some(stats) = volume_stats(&path) else {
            continue;
        };
        // "/Volumes/Macintosh HD" is a symlink to "/", and plain folders are not mounts.
        if stats.mounted_on.as_deref() != Some(mount_point.as_str()) || stats.total_bytes == 0 {
            continue;
        }
        volumes.push(VolumeInfo {
            name: entry.file_name().to_string_lossy().into_owned(),
            mount_point,
            file_system: stats.file_system,
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary: false,
        });
    }
    volumes
}

#[cfg(all(unix, not(target_os = "macos")))]
fn volumes() -> Vec<VolumeInfo> {
    let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut volumes = Vec::new();
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(_device), Some(mount_point), Some(file_system)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let mount_point = mount_point.replace("\\040", " ");
        let is_primary = mount_point == "/";
        let is_removable = ["/media/", "/mnt/", "/run/media/"]
            .iter()
            .any(|prefix| mount_point.starts_with(prefix));
        if !is_primary && !is_removable {
            continue;
        }
        let Some(stats) = volume_stats(Path::new(&mount_point)) else {
            continue;
        };
        if stats.total_bytes == 0 {
            continue;
        }
        volumes.push(VolumeInfo {
            name: if is_primary {
                startup_volume_name()
            } else {
                Path::new(&mount_point)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| mount_point.clone())
            },
            mount_point,
            file_system: Some(file_system.to_uppercase()),
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary,
        });
    }
    volumes.sort_by_key(|volume| !volume.is_primary);
    volumes
}

#[cfg(not(unix))]
fn volumes() -> Vec<VolumeInfo> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut length = 0usize;
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || length == 0 {
        return None;
    }
    let mut buffer = vec![0u8; length];
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    buffer.truncate(length);
    std::ffi::CStr::from_bytes_until_nul(&buffer)
        .ok()
        .map(|value| value.to_string_lossy().trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "macos")]
fn sysctl_u64(name: &std::ffi::CStr) -> Option<u64> {
    let mut value = 0u64;
    let mut length = std::mem::size_of::<u64>();
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && length == std::mem::size_of::<u64>()).then_some(value)
}

#[cfg(target_os = "macos")]
fn device_info() -> DeviceInfo {
    let name = Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| sysctl_string(c"kern.hostname"))
        .unwrap_or_else(|| "This Mac".into());
    DeviceInfo {
        name,
        chip: sysctl_string(c"machdep.cpu.brand_string"),
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
        memory_bytes: sysctl_u64(c"hw.memsize"),
    }
}

#[cfg(not(target_os = "macos"))]
fn device_info() -> DeviceInfo {
    let name = fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".into());
    let chip = fs::read_to_string("/proc/cpuinfo").ok().and_then(|info| {
        info.lines().find_map(|line| {
            line.strip_prefix("model name")
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, model)| model.trim().to_owned())
        })
    });
    let memory_bytes = fs::read_to_string("/proc/meminfo").ok().and_then(|info| {
        info.lines().find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|rest| {
                    rest.trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
                .map(|kilobytes| kilobytes * 1024)
        })
    });
    DeviceInfo {
        name,
        chip,
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
        memory_bytes,
    }
}

#[tauri::command]
async fn disk_overview() -> Result<DiskOverview, String> {
    tauri::async_runtime::spawn_blocking(|| DiskOverview {
        device: device_info(),
        volumes: volumes(),
        home: home_location().map(|home| home.path),
    })
    .await
    .map_err(|error| error.to_string())
}

/// Bytes a file really takes on disk (like `du`), counting hard-linked files once.
#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_dir() && metadata.nlink() > 1 && !seen.insert((metadata.dev(), metadata.ino()))
    {
        return 0;
    }
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata, _seen: &mut HashSet<(u64, u64)>) -> u64 {
    metadata.len()
}

#[cfg(unix)]
fn device_id(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.dev())
}

#[cfg(not(unix))]
fn device_id(_metadata: &fs::Metadata) -> Option<u64> {
    None
}

const SCAN_PROGRESS_EVERY_ENTRIES: u64 = 256;

/// Walks `path` without following symlinks or crossing into other volumes.
fn tree_usage(
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
            if visited % SCAN_PROGRESS_EVERY_ENTRIES == 0 {
                on_progress(total);
            }
        }
    }
    total
}

enum ScanEvent<'a> {
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
fn scan_folder_usage_tree(
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

/// Trash locations for the current platform. macOS and Linux keep per-user trash under the home
/// folder; Windows spreads it across `<drive>:\$Recycle.Bin` on every fixed drive; macOS also keeps
/// a copy of deleted files on each mounted volume under `.Trashes/<uid>`.
fn trash_paths() -> Vec<PathBuf> {
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
fn trash_dir_usage(path: &Path, seen: &mut HashSet<(u64, u64)>) -> Option<u64> {
    // The top-level read must succeed; otherwise we can’t measure this trash at all.
    fs::read_dir(path).ok()?;
    Some(tree_usage(path, None, seen, &mut |_| {}))
}

/// Sums the size of every trash location. Returns `None` when no trash location could be measured
/// (none exist, or every one of them is unreadable — usually a Full Disk Access problem), so the
/// caller can tell "can’t measure" apart from "measured and empty".
fn trash_bytes() -> Option<u64> {
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
fn empty_trash() -> Result<Option<u64>, String> {
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
fn trash_usage() -> Option<u64> {
    trash_bytes()
}

/// Opens System Settings on macOS, landing on the Full Disk Access pane so the user can grant
/// liteexplorer access to the Trash. Other platforms are a no-op for now.
#[tauri::command]
fn open_system_settings() -> Result<(), String> {
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
fn open_trash() -> Result<(), String> {
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
fn request_full_disk_access() {
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

fn usage_root(root: Option<String>) -> Result<String, String> {
    root.or_else(|| home_location().map(|home| home.path))
        .ok_or_else(|| "Couldn’t find your home folder.".into())
}

async fn save_folder_usage(
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

async fn load_folder_usage(
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
async fn folder_usage(
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

const SCAN_PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(120);

#[tauri::command]
fn scan_folder_usage(
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

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn os_detection() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// WKWebView throttles rendering updates (and `requestAnimationFrame`) to ~60fps
/// even on ProMotion displays. Turn off WebKit's `PreferPageRenderingUpdatesNear60FPSEnabled`
/// feature so the page renders at the display's native refresh rate (e.g. 120Hz).
///
/// This goes through private WebKit SPI (`_features`, `_setEnabled:forFeature:`), so every
/// selector is checked first and the call is skipped if WebKit no longer provides it.
#[cfg(target_os = "macos")]
fn unlock_webview_frame_rate(window: &tauri::WebviewWindow) {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject, Bool},
        sel,
    };
    use std::ffi::{c_char, CStr};

    let _ = window.with_webview(|webview| unsafe {
        let Some(preferences_class) = AnyClass::get(c"WKPreferences") else {
            return;
        };
        let responds: Bool = msg_send![preferences_class, respondsToSelector: sel!(_features)];
        if !responds.as_bool() {
            return;
        }

        let wk_webview = webview.inner() as *mut AnyObject;
        let configuration: *mut AnyObject = msg_send![wk_webview, configuration];
        let preferences: *mut AnyObject = msg_send![configuration, preferences];
        let can_set: Bool =
            msg_send![preferences, respondsToSelector: sel!(_setEnabled:forFeature:)];
        if preferences.is_null() || !can_set.as_bool() {
            return;
        }

        let features: *mut AnyObject = msg_send![preferences_class, _features];
        let count: usize = msg_send![features, count];
        for index in 0..count {
            let feature: *mut AnyObject = msg_send![features, objectAtIndex: index];
            let key: *mut AnyObject = msg_send![feature, key];
            let utf8: *const c_char = msg_send![key, UTF8String];
            if !utf8.is_null()
                && CStr::from_ptr(utf8).to_bytes() == b"PreferPageRenderingUpdatesNear60FPSEnabled"
            {
                let _: () = msg_send![preferences, _setEnabled: Bool::NO, forFeature: feature];
                break;
            }
        }
    });
}

fn remove_close_window<R: tauri::Runtime>(submenu: &Submenu<R>) -> tauri::Result<()> {
    for item in submenu.items()? {
        if let MenuItemKind::Predefined(predefined) = &item {
            let text = predefined.text()?.replace('&', "");
            if text == "Close Window" || text == "Close" {
                submenu.remove(predefined)?;
            }
        }
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
const CHECK_FOR_UPDATES: &str = "check-for-updates";

/// Rebuilds the default About item with the app icon, so the About panel shows it even when the
/// binary runs outside an app bundle (e.g. `tauri dev`), where macOS falls back to a folder icon.
fn set_about_icon<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    menu: &Menu<R>,
) -> tauri::Result<()> {
    let package = app.package_info();
    let bundle = &app.config().bundle;
    let about = PredefinedMenuItem::about(
        app,
        None,
        Some(AboutMetadata {
            name: Some("Lite Explorer".to_string()),
            version: Some(package.version.to_string()),
            copyright: bundle.copyright.clone(),
            authors: bundle.publisher.clone().map(|publisher| vec![publisher]),
            // `credits` is the description on macOS; `comments` covers Windows and Linux.
            credits: Some(env!("CARGO_PKG_DESCRIPTION").to_string()),
            comments: Some(env!("CARGO_PKG_DESCRIPTION").to_string()),
            icon: Some(tauri::include_image!("./icons/icon.png")),
            ..Default::default()
        }),
    )?;
    for entry in menu.items()? {
        let MenuItemKind::Submenu(submenu) = entry else {
            continue;
        };
        let index = submenu.items()?.iter().position(|child| match child {
            MenuItemKind::Predefined(predefined) => predefined
                .text()
                .map(|text| text.starts_with("About"))
                .unwrap_or(false),
            _ => false,
        });
        if let Some(index) = index {
            submenu.remove_at(index)?;
            return submenu.insert(&about, index);
        }
    }
    Ok(())
}

/// Puts "Check for Updates…" right after About: in the app menu on macOS, in Help elsewhere.
fn add_check_for_updates<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    menu: &Menu<R>,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(
        app,
        CHECK_FOR_UPDATES,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    for entry in menu.items()? {
        let MenuItemKind::Submenu(submenu) = entry else {
            continue;
        };
        let about = submenu.items()?.iter().position(|child| match child {
            MenuItemKind::Predefined(predefined) => predefined
                .text()
                .map(|text| text.starts_with("About"))
                .unwrap_or(false),
            _ => false,
        });
        if let Some(index) = about {
            return submenu.insert(&item, index + 1);
        }
    }
    let help = Submenu::with_items(app, "Help", true, &[&item])?;
    menu.append(&help)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                restore_window_state(&window);
                let window = window.clone();
                window.clone().on_window_event(move |event| {
                    if matches!(event, WindowEvent::CloseRequested { .. }) {
                        save_window_state(&window);
                    }
                });
            }
            let database = tauri::async_runtime::block_on(open_database(app.handle()))?;
            app.manage(Database(database));
            app.manage(remote::RemoteClients::default());
            app.manage(network::Mounts::default());
            app.manage(network::discovery::Discovery::default());
            app.manage(network::servers::Sessions::default());
            app.manage(transfer::TransferRegistry::default());
            app.manage(FolderScans(std::sync::Mutex::new(HashSet::new())));
            let floating = CheckMenuItem::with_id(
                app.handle(),
                "toggle-sidebar-floating",
                "Floating Sidebar",
                true,
                false,
                Some("CmdOrCtrl+Shift+F"),
            )?;
            let sidebar = Submenu::with_items(app.handle(), "Sidebar", true, &[&floating])?;
            let hidden_files = CheckMenuItem::with_id(
                app.handle(),
                "toggle-hidden-files",
                "Show Hidden Files",
                true,
                false,
                Some("CmdOrCtrl+Shift+Period"),
            )?;
            let show_fps = CheckMenuItem::with_id(
                app.handle(),
                "toggle-show-fps",
                "Show FPS",
                true,
                false,
                None::<&str>,
            )?;
            let menu = Menu::default(app.handle())?;
            let new_folder = MenuItem::with_id(
                app.handle(),
                "new-folder",
                "New Folder",
                true,
                Some("CmdOrCtrl+Shift+N"),
            )?;
            let new_file = MenuItem::with_id(
                app.handle(),
                "new-file",
                "New File",
                true,
                Some("CmdOrCtrl+Shift+Alt+N"),
            )?;
            let open_item = MenuItem::with_id(app.handle(), "open", "Open", false, None::<&str>)?;
            let go_to_folder = MenuItem::with_id(
                app.handle(),
                "go-to-folder",
                "Go to Folder…",
                true,
                Some("CmdOrCtrl+Shift+P"),
            )?;
            let new_tab = MenuItem::with_id(
                app.handle(),
                "new-tab",
                "New Tab",
                true,
                Some("CmdOrCtrl+T"),
            )?;
            let close_tab = MenuItem::with_id(
                app.handle(),
                "close-tab",
                "Close Tab",
                true,
                Some("CmdOrCtrl+W"),
            )?;
            let next_tab = MenuItem::with_id(
                app.handle(),
                "next-tab",
                "Show Next Tab",
                true,
                Some("CmdOrCtrl+Shift+]"),
            )?;
            let previous_tab = MenuItem::with_id(
                app.handle(),
                "previous-tab",
                "Show Previous Tab",
                true,
                Some("CmdOrCtrl+Shift+["),
            )?;
            let new_tab_other = MenuItem::with_id(
                app.handle(),
                "new-tab-other",
                "New Tab in Second Pane",
                true,
                Some("CmdOrCtrl+Shift+T"),
            )?;
            let show_second_pane = MenuItem::with_id(
                app.handle(),
                "toggle-second-pane",
                "Show Second Pane",
                true,
                Some("CmdOrCtrl+Shift+L"),
            )?;
            let split_orientation = MenuItem::with_id(
                app.handle(),
                "toggle-pane-orientation",
                "Split Horizontally/Vertically",
                true,
                None::<&str>,
            )?;
            set_about_icon(app.handle(), &menu)?;
            add_check_for_updates(app.handle(), &menu)?;
            let items = menu.items()?;
            let file_index = items.iter().position(|item| match item {
                MenuItemKind::Submenu(submenu)
                    if submenu.text().ok().as_deref() == Some("File") =>
                {
                    true
                }
                _ => false,
            });
            match file_index {
                Some(index) => {
                    if let MenuItemKind::Submenu(file_submenu) = &items[index] {
                        remove_close_window(file_submenu)?;
                        file_submenu.insert(&new_tab, 0)?;
                        file_submenu.insert(&new_tab_other, 1)?;
                        file_submenu.insert(&new_folder, 2)?;
                        file_submenu.insert(&new_file, 3)?;
                        file_submenu.insert(&PredefinedMenuItem::separator(app.handle())?, 4)?;
                        file_submenu.insert(&open_item, 5)?;
                        file_submenu.insert(&go_to_folder, 6)?;
                        file_submenu.append(&PredefinedMenuItem::separator(app.handle())?)?;
                        file_submenu.append(&close_tab)?;
                    }
                }
                None => {
                    let file_menu = Submenu::with_items(
                        app.handle(),
                        "File",
                        true,
                        &[&new_tab, &new_tab_other, &new_folder, &new_file, &close_tab],
                    )?;
                    let view_index = items.iter().position(|item| match item {
                        MenuItemKind::Submenu(submenu)
                            if submenu.text().ok().as_deref() == Some("View") =>
                        {
                            true
                        }
                        _ => false,
                    });
                    match view_index {
                        Some(index) => menu.insert(&file_menu, index)?,
                        None => menu.append(&file_menu)?,
                    }
                }
            }
            let view_menu = menu.items()?.into_iter().find_map(|item| match item {
                MenuItemKind::Submenu(submenu)
                    if submenu.text().ok().as_deref() == Some("View") =>
                {
                    Some(submenu)
                }
                _ => None,
            });
            match view_menu {
                Some(view_menu) => {
                    view_menu.prepend(&hidden_files)?;
                    view_menu.insert(&show_fps, 1)?;
                    view_menu.insert(&sidebar, 2)?;
                    view_menu.insert(&show_second_pane, 3)?;
                    view_menu.insert(&split_orientation, 4)?;
                    view_menu.insert(&PredefinedMenuItem::separator(app.handle())?, 5)?;
                }
                None => menu.append(&Submenu::with_items(
                    app.handle(),
                    "View",
                    true,
                    &[
                        &hidden_files,
                        &show_fps,
                        &sidebar,
                        &show_second_pane,
                        &split_orientation,
                    ],
                )?)?,
            }
            let window_menu = menu.items()?.into_iter().find_map(|item| match item {
                MenuItemKind::Submenu(submenu)
                    if submenu.text().ok().as_deref() == Some("Window") =>
                {
                    Some(submenu)
                }
                _ => None,
            });
            match window_menu {
                Some(window_menu) => {
                    remove_close_window(&window_menu)?;
                    window_menu.append(&PredefinedMenuItem::separator(app.handle())?)?;
                    window_menu.append(&next_tab)?;
                    window_menu.append(&previous_tab)?;
                }
                None => menu.append(&Submenu::with_items(
                    app.handle(),
                    "Window",
                    true,
                    &[&next_tab, &previous_tab],
                )?)?,
            }
            app.manage(SidebarMenu(floating));
            app.manage(HiddenFilesMenu(hidden_files));
            app.manage(ShowFpsMenu(show_fps));
            app.manage(OpenMenuItem(open_item));
            app.manage(OpenTarget(std::sync::Mutex::new(None)));
            #[cfg(debug_assertions)]
            {
                let developer_tools = MenuItem::with_id(
                    app.handle(),
                    "open-dev-tools",
                    "Developer Tools",
                    true,
                    None::<&str>,
                )?;
                let prototype_switcher = CheckMenuItem::with_id(
                    app.handle(),
                    "toggle-prototype-switcher",
                    "Show Prototype Switcher",
                    true,
                    false,
                    None::<&str>,
                )?;
                let switcher_submenu = Submenu::with_items(
                    app.handle(),
                    "Prototype Switcher",
                    true,
                    &[&prototype_switcher],
                )?;
                let debug_menu = Submenu::with_items(
                    app.handle(),
                    "Debug",
                    true,
                    &[&developer_tools, &switcher_submenu],
                )?;
                menu.append(&debug_menu)?;
                app.manage(PrototypeSwitcherMenu(prototype_switcher));
            }
            app.set_menu(menu)?;
            #[cfg(target_os = "macos")]
            for window in app.webview_windows().values() {
                unlock_webview_frame_rate(window);
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            #[cfg(debug_assertions)]
            {
                if event.id() == "open-dev-tools" {
                    if let Some(window) = app.get_webview_window("main") {
                        window.open_devtools();
                    }
                    return;
                }
                if event.id() == "toggle-prototype-switcher" {
                    let visible = app
                        .state::<PrototypeSwitcherMenu>()
                        .0
                        .is_checked()
                        .unwrap_or(false);
                    let _ = app.emit("dev-tools", visible);
                    return;
                }
            }
            if event.id() == "toggle-sidebar-floating" {
                let floating = app.state::<SidebarMenu>().0.is_checked().unwrap_or(false);
                let _ = app.emit("sidebar-floating", floating);
            } else if event.id() == "toggle-hidden-files" {
                let show = app
                    .state::<HiddenFilesMenu>()
                    .0
                    .is_checked()
                    .unwrap_or(false);
                let _ = app.emit("show-hidden-files", show);
            } else if event.id() == "toggle-show-fps" {
                let show = app.state::<ShowFpsMenu>().0.is_checked().unwrap_or(false);
                let _ = app.emit("show-fps", show);
            } else if event.id() == "new-folder" {
                let _ = app.emit("request-create-folder", ());
            } else if event.id() == "new-file" {
                let _ = app.emit("request-create-file", ());
            } else if event.id() == "new-tab" {
                let _ = app.emit("tab-new", ());
            } else if event.id() == "new-tab-other" {
                let _ = app.emit("tab-new-other", ());
            } else if event.id() == "toggle-second-pane" {
                let _ = app.emit("pane-toggle", ());
            } else if event.id() == "toggle-pane-orientation" {
                let _ = app.emit("pane-orientation", ());
            } else if event.id() == "close-tab" {
                let _ = app.emit("tab-close", ());
            } else if event.id() == "next-tab" {
                let _ = app.emit("tab-next", ());
            } else if event.id() == "previous-tab" {
                let _ = app.emit("tab-prev", ());
            } else if event.id() == "open" {
                let target = app.state::<OpenTarget>().0.lock().unwrap().clone();
                if let Some((path, _)) = target {
                    let _ = app.emit("request-open", path);
                }
            } else if event.id() == "go-to-folder" {
                let _ = app.emit("command-palette", ());
            } else if event.id() == CHECK_FOR_UPDATES {
                let _ = app.emit("check-for-updates", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            os_detection,
            set_sidebar_floating,
            set_show_hidden_files,
            set_show_fps,
            locations,
            remote::test_remote_location,
            remote::add_remote_location,
            remote::remove_remote_location,
            network::test_network_location,
            network::add_network_location,
            network::update_network_location,
            network::remove_network_location,
            network::network_location,
            network::connect_network_location,
            network::disconnect_network_location,
            network::network_connections,
            network::mount_network_share,
            network::trust_network_host,
            network::discovery::scan_smb_servers,
            network::discovery::stop_network_scan,
            network::discovery::open_local_network_settings,
            remote::download_remote_file,
            remote::write::delete_remote_items,
            remote::write::upload_remote_files,
            remote::write::download_remote_items,
            transfer::cancel_transfer,
            archive::create_archive,
            archive::extract_archive,
            remote::buckets::create_remote_bucket,
            remote::buckets::delete_remote_bucket,
            remote::buckets::remote_provider,
            remote::bucket_settings::bucket_settings,
            remote::bucket_settings::set_bucket_versioning,
            remote::bucket_settings::set_bucket_public,
            favorites,
            read_directory,
            create_item,
            rename_item,
            open_with_apps,
            open_with,
            open_path,
            reveal_path,
            default_app,
            set_open_target,
            compute_directory_sizes,
            read_file_preview,
            record_recent,
            recents,
            clear_recents,
            copy_item,
            move_item,
            trash_item,
            delete_item,
            disk_overview,
            folder_usage,
            scan_folder_usage,
            empty_trash,
            open_system_settings,
            request_full_disk_access,
            open_trash,
            trash_usage
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
struct SidebarMenu(CheckMenuItem<tauri::Wry>);

#[tauri::command]
fn set_sidebar_floating(menu: State<'_, SidebarMenu>, floating: bool) {
    let _ = menu.0.set_checked(floating);
}

struct HiddenFilesMenu(CheckMenuItem<tauri::Wry>);

#[tauri::command]
fn set_show_hidden_files(menu: State<'_, HiddenFilesMenu>, show: bool) {
    let _ = menu.0.set_checked(show);
}

struct ShowFpsMenu(CheckMenuItem<tauri::Wry>);

#[tauri::command]
fn set_show_fps(menu: State<'_, ShowFpsMenu>, show: bool) {
    let _ = menu.0.set_checked(show);
}

#[cfg(debug_assertions)]
struct PrototypeSwitcherMenu(CheckMenuItem<tauri::Wry>);

struct OpenMenuItem(MenuItem<tauri::Wry>);
struct OpenTarget(std::sync::Mutex<Option<(String, bool)>>);

#[tauri::command]
fn set_open_target(app: AppHandle, path: String, is_directory: bool, enabled: bool) {
    let open_menu = &app.state::<OpenMenuItem>().0;
    let text = if enabled {
        if is_directory {
            "Open Folder"
        } else {
            "Open File"
        }
    } else {
        "Open"
    };
    let _ = open_menu.set_text(text);
    let _ = open_menu.set_enabled(enabled);
    *app.state::<OpenTarget>().0.lock().unwrap() = if enabled && !path.is_empty() {
        Some((path, is_directory))
    } else {
        None
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn window_state_is_clamped_to_the_visible_work_area() {
        let (position, size) = clamp_window_state(
            &WindowState {
                x: 3_000,
                y: -200,
                width: 2_000,
                height: 1_000,
            },
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1_440, 900),
        );
        assert_eq!(position, PhysicalPosition::new(0, 0));
        assert_eq!(size, PhysicalSize::new(1_440, 900));
    }

    #[test]
    fn permanent_delete_removes_a_directory_tree() {
        let path = std::env::temp_dir().join(format!("liteexplorer-delete-{}", nanos()));
        fs::create_dir_all(path.join("nested")).unwrap();
        fs::write(path.join("nested/file.txt"), "remove me").unwrap();

        delete_item_path(&path).unwrap();

        assert!(!path.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn app_icon_is_a_png_data_url() {
        let icon = app_icon_data_url("/System/Applications/Calculator.app").unwrap();
        assert!(icon.starts_with("data:image/png;base64,"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_app_resolves_for_a_text_file() {
        let path = std::env::temp_dir().join("liteexplorer-default-app.txt");
        std::fs::write(&path, "hello").unwrap();
        let app = default_app(path.to_string_lossy().into_owned()).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(app.path.ends_with(".app"));
        assert!(!app.name.is_empty() && !app.name.ends_with(".app"));
    }

    #[test]
    fn migration_handles_an_existing_locations_table() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            sqlx::query("CREATE TABLE locations (path TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, position INTEGER NOT NULL)")
                .execute(&pool).await.unwrap();
            apply_migrations(&pool).await.unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations")
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                sqlx::migrate!("./migrations").iter().count() as i64
            );
        });
    }

    #[test]
    fn recents_returns_only_entries_within_thirty_days() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE recents (path TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, opened_at INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            let now = now_secs();
            for (path, opened_at) in [
                ("/recent", now - 3 * 24 * 3600),
                ("/old", now - 40 * 24 * 3600),
                ("/recent2", now - 1 * 24 * 3600),
            ] {
                sqlx::query(
                    "INSERT INTO recents (path, name, kind, opened_at) VALUES (?, ?, ?, ?) \
                     ON CONFLICT(path) DO UPDATE SET name = excluded.name, kind = excluded.kind, opened_at = excluded.opened_at",
                )
                .bind(path)
                .bind(path.trim_start_matches('/'))
                .bind("folder")
                .bind(opened_at)
                .execute(&pool)
                .await
                .unwrap();
            }
            let cutoff = now - THIRTY_DAYS_SECS;
            let rows = sqlx::query(
                "SELECT path FROM recents WHERE opened_at >= ? ORDER BY opened_at DESC",
            )
            .bind(cutoff)
            .fetch_all(&pool)
            .await
            .unwrap();
            let paths: Vec<String> = rows.iter().map(|row| row.get("path")).collect();
            assert_eq!(paths, vec!["/recent2".to_string(), "/recent".to_string()]);
        });
    }

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
        assert!(entries
            .iter()
            .any(|entry| entry.name == "file.txt" && !entry.is_directory && !entry.is_hidden));
        assert!(entries
            .iter()
            .any(|entry| entry.name == ".hidden" && entry.is_hidden));
        fs::remove_dir_all(directory).unwrap();
    }

    fn preview_fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "liteexplorer-preview-{}-{}",
            name,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn file_preview_reads_text_files() {
        let path = preview_fixture("notes.txt", b"hello\nworld");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.name, "notes.txt");
        assert_eq!(preview.size, 11);
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("hello\nworld"));
        assert!(!preview.truncated);
        assert!(preview.modified.is_some());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_detects_binary_files_by_nul_byte() {
        let path = preview_fixture("blob.bin", b"\x89PNG\r\n\x1a\n\x00\x00");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Binary);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_reads_empty_files_as_text() {
        let path = preview_fixture("empty.txt", b"");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some(""));
        assert_eq!(preview.size, 0);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_encodes_images_as_data_url() {
        let path = preview_fixture("photo.png", b"\x89PNG\r\n\x1a\n");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Image);
        assert_eq!(preview.content, None);
        assert!(preview.src.unwrap().starts_with("data:image/png;base64,"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_decodes_non_utf8_text_lossily() {
        let path = preview_fixture("latin1.txt", b"caf\xe9");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("caf\u{FFFD}"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_truncates_large_files_on_a_utf8_boundary() {
        let mut bytes = vec![b'a'; PREVIEW_MAX_BYTES - 1];
        bytes.extend_from_slice("é".as_bytes());
        bytes.push(b'b');
        let path = preview_fixture("large.txt", &bytes);

        let preview = file_preview(&path).unwrap();

        assert!(preview.truncated);
        let content = preview.content.unwrap();
        assert_eq!(content.len(), PREVIEW_MAX_BYTES - 1);
        assert!(!content.contains('\u{FFFD}'));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_returns_directory_metadata_without_content() {
        let path = preview_fixture("inner.txt", b"x");
        let directory = path.parent().unwrap();

        let preview = file_preview(directory).unwrap();

        assert_eq!(preview.kind, PreviewKind::Directory);
        assert_eq!(preview.content, None);
        assert!(!preview.truncated);
        fs::remove_dir_all(directory).unwrap();
    }

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
            apply_migrations(&pool).await.unwrap();

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

    #[test]
    fn file_preview_errors_for_missing_files() {
        assert!(file_preview(Path::new("/definitely/missing/liteexplorer.txt")).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn detects_installed_icloud_and_google_drive_locations() {
        let home = std::env::temp_dir().join(format!("liteexplorer-cloud-{}", nanos()));
        fs::create_dir_all(home.join("Library/Mobile Documents/com~apple~CloudDocs")).unwrap();
        fs::create_dir_all(home.join("Library/CloudStorage/GoogleDrive-work")).unwrap();
        fs::create_dir_all(home.join("Library/CloudStorage/GoogleDrive-personal")).unwrap();

        let locations = macos_cloud_locations(&home);

        assert!(locations
            .iter()
            .any(|location| location.name == "iCloud Drive"));
        assert!(locations
            .iter()
            .any(|location| location.name == "Google Drive (work)"));
        assert!(locations
            .iter()
            .any(|location| location.name == "Google Drive (personal)"));
        fs::remove_dir_all(home).unwrap();
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

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    #[test]
    fn copy_item_duplicates_trees_and_records_recents() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE recents (path TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, opened_at INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let source = std::env::temp_dir().join(format!("liteexplorer-copy-src-{}", nanos()));
            fs::create_dir_all(source.join("nested")).unwrap();
            fs::write(source.join("nested/data.txt"), "hi").unwrap();

            let dest = std::env::temp_dir().join(format!("liteexplorer-copy-dest-{}", nanos()));
            fs::create_dir_all(&dest).unwrap();

            let entry = copy_item_inner(
                &Database(pool.clone()),
                &source.to_string_lossy().into_owned(),
                &dest.to_string_lossy().into_owned(),
            )
            .await
            .unwrap();
            assert!(entry.is_directory);
            assert!(dest
                .join(source.file_name().unwrap())
                .join("nested/data.txt")
                .exists());

            let rows = sqlx::query_scalar::<_, String>("SELECT path FROM recents")
                .fetch_all(&pool)
                .await
                .unwrap();
            assert!(rows.contains(&entry.path));

            fs::remove_dir_all(&source).unwrap();
            fs::remove_dir_all(&dest).unwrap();
        });
    }

    #[test]
    fn move_item_relocates_and_records_recents() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            sqlx::query(
                "CREATE TABLE recents (path TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL, opened_at INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();

            let source = std::env::temp_dir().join(format!("liteexplorer-move-src-{}", nanos()));
            fs::write(&source, "payload").unwrap();

            let dest = std::env::temp_dir().join(format!("liteexplorer-move-dest-{}", nanos()));
            fs::create_dir_all(&dest).unwrap();

            let entry = move_item_inner(
                &Database(pool.clone()),
                &source.to_string_lossy().into_owned(),
                &dest.to_string_lossy().into_owned(),
            )
            .await
            .unwrap();
            assert!(!entry.is_directory);
            assert!(!source.exists());
            assert!(dest.join(source.file_name().unwrap()).exists());

            let rows = sqlx::query_scalar::<_, String>("SELECT path FROM recents")
                .fetch_all(&pool)
                .await
                .unwrap();
            assert!(rows.contains(&entry.path));

            fs::remove_dir_all(&dest).unwrap();
        });
    }
}
