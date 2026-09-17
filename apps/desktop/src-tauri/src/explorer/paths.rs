use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::app::db::Database;
use crate::{network, remote};

#[derive(Serialize)]
pub struct Location {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) kind: String,
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub const THIRTY_DAYS_SECS: i64 = 30 * 24 * 3600;

/// The user's home folder: `USERPROFILE` on Windows, `HOME` elsewhere.
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(target_os = "windows") {
        "USERPROFILE"
    } else {
        "HOME"
    })
    .map(PathBuf::from)
}

/// Expands a leading `~` to `home`, leaving every other path untouched.
pub fn expand_tilde_with(path: &str, home: Option<&Path>) -> PathBuf {
    let Some(home) = home else {
        return PathBuf::from(path);
    };
    if path == "~" {
        return home.to_path_buf();
    }
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\"));
    match rest {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

/// Expands a leading `~` to the user's home folder.
pub fn expand_tilde(path: &str) -> PathBuf {
    expand_tilde_with(path, home_dir().as_deref())
}

pub fn home_location() -> Option<Location> {
    let path = home_dir()?;
    Some(Location {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: "home".into(),
    })
}

pub fn startup_volume_name() -> String {
    #[cfg(target_os = "macos")]
    {
        Command::new("diskutil")
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
            .unwrap_or_else(|| "Macintosh HD".into())
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

pub fn system_locations() -> Vec<Location> {
    #[cfg(not(target_os = "macos"))]
    {
        let mut locations: Vec<Location> = crate::system::volumes::volumes()
            .into_iter()
            .map(|volume| Location {
                name: volume.name,
                path: volume.mount_point,
                kind: "volume".into(),
            })
            .collect();
        if let Some(home) = home_location() {
            locations.push(home);
        }
        return locations;
    }

    #[cfg(target_os = "macos")]
    {
        let mut locations = vec![Location {
            name: startup_volume_name(),
            path: "/".into(),
            kind: "volume".into(),
        }];
        if let Some(home) = home_location() {
            locations.push(home);
        }
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            locations.extend(macos_cloud_locations(&home));
        }
        locations
    }
}

/// Cloud providers installed by macOS expose ordinary directories. Keeping them as local
/// locations lets their own File Provider handle syncing and on-demand downloads.
#[cfg(target_os = "macos")]
pub fn macos_cloud_locations(home: &Path) -> Vec<Location> {
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

pub fn location(path: PathBuf) -> Option<Location> {
    Some(Location {
        name: path.file_name()?.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        kind: "folder".into(),
    })
}

pub fn add_location(locations: &mut Vec<Location>, seen: &mut HashSet<PathBuf>, path: PathBuf) {
    if path.is_dir() && seen.insert(path.clone()) {
        if let Some(location) = location(path) {
            locations.push(location);
        }
    }
}

pub fn file_url_path(value: &str) -> Option<PathBuf> {
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

#[tauri::command]
pub async fn locations(database: State<'_, Database>) -> Result<Vec<Location>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn expand_tilde_replaces_a_leading_tilde() {
        let home = Path::new("/Users/me");
        assert_eq!(
            expand_tilde_with("~", Some(home)),
            PathBuf::from("/Users/me")
        );
        assert_eq!(
            expand_tilde_with("~/Work/lp", Some(home)),
            PathBuf::from("/Users/me/Work/lp")
        );
        assert_eq!(
            expand_tilde_with("/abs/path", Some(home)),
            PathBuf::from("/abs/path")
        );
        assert_eq!(
            expand_tilde_with("relative/path", Some(home)),
            PathBuf::from("relative/path")
        );
        assert_eq!(expand_tilde_with("~", None), PathBuf::from("~"));
    }

    fn nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
}
