use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::State;
#[cfg(target_os = "macos")]
use trash::macos::{DeleteMethod, TrashContextExtMacos};

use crate::app::db::Database;
use crate::explorer::entries::{
    coordinated_read, coordinated_write, single_entry, unique_name, DirectoryEntry,
};
use crate::explorer::recents::{insert_recent, recent_kind};
use crate::{network, remote};

#[tauri::command]
pub async fn create_item(
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

pub fn create_local_item(
    parent: String,
    kind: String,
    name: String,
) -> Result<DirectoryEntry, String> {
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
pub async fn rename_item(
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

pub fn rename_local_item(path: String, new_name: String) -> Result<DirectoryEntry, String> {
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

#[tauri::command]
pub async fn copy_item(
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

pub async fn copy_item_inner(
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
pub async fn move_item(
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

pub async fn move_item_inner(
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
pub async fn trash_item(path: String) -> Result<(), String> {
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

pub fn trash_local_item(path: &Path) -> Result<(), String> {
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
pub async fn delete_item(path: String) -> Result<(), String> {
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

pub fn delete_item_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|error| error.to_string())
}

pub fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn permanent_delete_removes_a_directory_tree() {
        let path = std::env::temp_dir().join(format!("liteexplorer-delete-{}", nanos()));
        fs::create_dir_all(path.join("nested")).unwrap();
        fs::write(path.join("nested/file.txt"), "remove me").unwrap();

        delete_item_path(&path).unwrap();

        assert!(!path.exists());
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
                &source.to_string_lossy(),
                &dest.to_string_lossy(),
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
                &source.to_string_lossy(),
                &dest.to_string_lossy(),
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
