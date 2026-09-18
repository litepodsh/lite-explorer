use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::app::db::Database;
use crate::explorer::paths::{now_secs, THIRTY_DAYS_SECS};

#[derive(Serialize)]
pub struct Recent {
    name: String,
    path: String,
    kind: String,
    opened_at: i64,
}

pub fn recent_kind(is_directory: bool) -> String {
    if is_directory {
        "folder".into()
    } else {
        "file".into()
    }
}

pub async fn insert_recent(
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
pub async fn record_recent(
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
pub async fn recents(database: State<'_, Database>) -> Result<Vec<Recent>, String> {
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
pub async fn clear_recents(database: State<'_, Database>) -> Result<(), String> {
    sqlx::query("DELETE FROM recents")
        .execute(&database.0)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

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
                ("/recent2", now - 24 * 3600),
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
}
