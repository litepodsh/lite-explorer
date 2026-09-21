use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
};

use sqlx::{Row, SqlitePool};
use tauri::State;

use crate::app::db::Database;
use crate::explorer::paths::{add_location, file_url_path, home_dir, location, Location};
use crate::{network, remote};

pub fn standard_favorites() -> Vec<Location> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let mut locations = Vec::new();
    let mut seen = HashSet::new();
    // Folders that don't exist are skipped, so macOS "Movies" and Windows "Videos" can share a list.
    for name in [
        "Desktop",
        "Documents",
        "Downloads",
        "Movies",
        "Videos",
        "Music",
        "Pictures",
    ] {
        add_location(&mut locations, &mut seen, home.join(name));
    }
    locations
}

#[cfg(target_os = "macos")]
pub fn macos_favorites() -> Vec<Location> {
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
pub fn linux_favorites() -> Vec<Location> {
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
        if let Ok(contents) = std::fs::read_to_string(bookmarks) {
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

pub fn system_favorites() -> Vec<Location> {
    #[cfg(target_os = "macos")]
    {
        macos_favorites()
    }
    #[cfg(target_os = "linux")]
    {
        linux_favorites()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        standard_favorites()
    }
}

/// A row of the `favorites` table. `source` is `system` for folders read from the OS
/// sidebar and `user` for folders added in the app; `hidden` hides a system favorite
/// without touching the OS list.
pub struct StoredFavorite {
    name: String,
    path: String,
    source: String,
    hidden: bool,
    position: i64,
}

fn favorite_kind(path: &str) -> String {
    network::parse_network_path(path)
        .map(|network| network::to_db(network.protocol))
        .unwrap_or_else(|| {
            if remote::is_remote_path(path) {
                "s3"
            } else {
                "folder"
            }
            .into()
        })
}

fn favorite_name(path: &str) -> Option<String> {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .find(|part| !part.is_empty())
        .map(str::to_string)
}

/// Favorites shown in the sidebar: not hidden, still an OS favorite unless added in
/// the app, and still an existing local folder or a connected location. Ordered by position.
pub fn visible_favorites(
    stored: &[StoredFavorite],
    system: &[Location],
    exists: impl Fn(&str) -> bool,
) -> Vec<Location> {
    let system: HashSet<&str> = system
        .iter()
        .map(|location| location.path.as_str())
        .collect();
    let mut visible: Vec<&StoredFavorite> = stored
        .iter()
        .filter(|favorite| {
            !favorite.hidden
                && (favorite.source == "user" || system.contains(favorite.path.as_str()))
                && (remote::is_remote_path(&favorite.path)
                    || network::is_network_path(&favorite.path)
                    || exists(&favorite.path))
        })
        .collect();
    visible.sort_by_key(|favorite| favorite.position);
    visible
        .into_iter()
        .map(|favorite| Location {
            name: favorite.name.clone(),
            path: favorite.path.clone(),
            kind: favorite_kind(&favorite.path),
        })
        .collect()
}

pub async fn load_system_favorites() -> Result<Vec<Location>, String> {
    tauri::async_runtime::spawn_blocking(system_favorites)
        .await
        .map_err(|error| error.to_string())
}

pub async fn stored_favorites(pool: &SqlitePool) -> Result<Vec<StoredFavorite>, String> {
    sqlx::query("SELECT name, path, source, hidden, position FROM favorites")
        .fetch_all(pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| StoredFavorite {
                    name: row.get("name"),
                    path: row.get("path"),
                    source: row.get("source"),
                    hidden: row.get::<i64, _>("hidden") != 0,
                    position: row.get("position"),
                })
                .collect()
        })
        .map_err(|error| error.to_string())
}

/// Stores OS favorites that are new since the last sync at the end of the list, then
/// returns the visible favorites. Existing rows keep their position and hidden flag.
pub async fn sync_favorites(
    pool: &SqlitePool,
    system: &[Location],
) -> Result<Vec<Location>, String> {
    for location in system {
        sqlx::query(
            "INSERT INTO favorites (path, name, source, hidden, position) \
             VALUES (?, ?, 'system', 0, (SELECT COALESCE(MAX(position), -1) + 1 FROM favorites)) \
             ON CONFLICT(path) DO NOTHING",
        )
        .bind(&location.path)
        .bind(&location.name)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    }
    let stored = stored_favorites(pool).await?;
    Ok(visible_favorites(&stored, system, |path| {
        Path::new(path).is_dir()
    }))
}

pub async fn write_favorite_order(pool: &SqlitePool, paths: &[String]) -> Result<(), String> {
    for (position, path) in paths.iter().enumerate() {
        sqlx::query("UPDATE favorites SET position = ? WHERE path = ?")
            .bind(position as i64)
            .bind(path)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn favorites(database: State<'_, Database>) -> Result<Vec<Location>, String> {
    let system = load_system_favorites().await?;
    sync_favorites(&database.0, &system).await
}

/// Adds a local or connected folder to the favorites at `index` (the end by default).
/// Adding a hidden or existing favorite shows it again and moves it to `index`.
#[tauri::command]
pub async fn add_favorite(
    path: String,
    index: Option<usize>,
    database: State<'_, Database>,
) -> Result<Vec<Location>, String> {
    const NOT_FOLDER: &str = "Only folders can be favorites.";
    let connected = remote::is_remote_path(&path) || network::is_network_path(&path);
    let name = if connected {
        favorite_name(&path).ok_or(NOT_FOLDER)?
    } else {
        let folder = PathBuf::from(&path);
        if !folder.is_dir() {
            return Err(NOT_FOLDER.into());
        }
        location(folder).ok_or(NOT_FOLDER)?.name
    };
    let system = load_system_favorites().await?;
    let mut order: Vec<String> = sync_favorites(&database.0, &system)
        .await?
        .into_iter()
        .map(|location| location.path)
        .filter(|favorite| favorite != &path)
        .collect();
    sqlx::query(
        "INSERT INTO favorites (path, name, source, hidden, position) VALUES (?, ?, 'user', 0, 0) \
         ON CONFLICT(path) DO UPDATE SET source = 'user', hidden = 0",
    )
    .bind(&path)
    .bind(&name)
    .execute(&database.0)
    .await
    .map_err(|error| error.to_string())?;
    order.insert(index.unwrap_or(order.len()).min(order.len()), path);
    write_favorite_order(&database.0, &order).await?;
    sync_favorites(&database.0, &system).await
}

/// Removes a favorite. OS favorites are only hidden, so the OS sidebar is left alone
/// and the folder doesn't come back on the next sync.
#[tauri::command]
pub async fn remove_favorite(
    path: String,
    database: State<'_, Database>,
) -> Result<Vec<Location>, String> {
    let system = load_system_favorites().await?;
    let query = if system.iter().any(|location| location.path == path) {
        "UPDATE favorites SET hidden = 1 WHERE path = ?"
    } else {
        "DELETE FROM favorites WHERE path = ?"
    };
    sqlx::query(query)
        .bind(&path)
        .execute(&database.0)
        .await
        .map_err(|error| error.to_string())?;
    sync_favorites(&database.0, &system).await
}

#[tauri::command]
pub async fn reorder_favorites(
    paths: Vec<String>,
    database: State<'_, Database>,
) -> Result<Vec<Location>, String> {
    write_favorite_order(&database.0, &paths).await?;
    let system = load_system_favorites().await?;
    sync_favorites(&database.0, &system).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::paths::now_secs;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::fs;

    fn stored(path: &str, source: &str, hidden: bool, position: i64) -> StoredFavorite {
        StoredFavorite {
            name: favorite_name(path).unwrap_or_else(|| path.into()),
            path: path.into(),
            source: source.into(),
            hidden,
            position,
        }
    }

    fn system_location(path: &str) -> Location {
        Location {
            name: path.trim_start_matches('/').into(),
            path: path.into(),
            kind: "folder".into(),
        }
    }

    #[test]
    fn visible_favorites_filters_and_orders_rows() {
        let stored = [
            stored("/docs", "system", false, 2),
            stored("/hidden", "system", true, 0),
            stored("/gone-from-os", "system", false, 1),
            stored("/mine", "user", false, 0),
            stored("/deleted", "user", false, 3),
        ];
        let system = [
            system_location("/docs"),
            system_location("/hidden"),
            system_location("/deleted"),
        ];
        let visible = visible_favorites(&stored, &system, |path| path != "/deleted");
        let paths: Vec<&str> = visible
            .iter()
            .map(|location| location.path.as_str())
            .collect();
        assert_eq!(paths, ["/mine", "/docs"]);
    }

    #[test]
    fn connected_favorites_stay_visible_and_keep_their_kind() {
        let stored = [
            stored("smb://server/share", "user", false, 0),
            stored("s3://account/bucket/reports/", "user", false, 1),
        ];
        let visible = visible_favorites(&stored, &[], |_| false);
        assert_eq!(
            visible
                .iter()
                .map(|location| (location.name.as_str(), location.kind.as_str()))
                .collect::<Vec<_>>(),
            [("share", "smb"), ("reports", "s3")]
        );
    }

    #[test]
    fn sync_favorites_appends_new_system_folders_and_keeps_order() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            crate::app::db::apply_migrations(&pool).await.unwrap();
            let root = std::env::temp_dir().join(format!("lite-favorites-{}", now_secs()));
            let folders: Vec<String> = ["a", "b", "c"]
                .iter()
                .map(|name| {
                    let folder = root.join(name);
                    fs::create_dir_all(&folder).unwrap();
                    folder.to_string_lossy().into_owned()
                })
                .collect();
            let system: Vec<Location> = folders[..2]
                .iter()
                .map(|path| system_location(path))
                .collect();
            sync_favorites(&pool, &system).await.unwrap();
            write_favorite_order(&pool, &[folders[1].clone(), folders[0].clone()])
                .await
                .unwrap();
            let system: Vec<Location> = folders.iter().map(|path| system_location(path)).collect();
            let visible = sync_favorites(&pool, &system).await.unwrap();
            let paths: Vec<&str> = visible
                .iter()
                .map(|location| location.path.as_str())
                .collect();
            assert_eq!(paths, [&folders[1], &folders[0], &folders[2]]);
            fs::remove_dir_all(root).unwrap();
        });
    }
}
