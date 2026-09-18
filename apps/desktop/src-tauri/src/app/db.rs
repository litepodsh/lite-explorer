use std::fs;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use tauri::Manager;

use crate::explorer::paths::system_locations;

pub struct Database(pub(crate) SqlitePool);

pub async fn open_database(
    app: &tauri::AppHandle,
) -> Result<SqlitePool, Box<dyn std::error::Error>> {
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
    let system = system_locations();
    for (position, location) in system.iter().enumerate() {
        sqlx::query("INSERT INTO locations (path, name, kind, position) VALUES (?, ?, ?, ?) ON CONFLICT(path) DO UPDATE SET name = excluded.name, kind = excluded.kind, position = excluded.position")
            .bind(&location.path).bind(&location.name).bind(&location.kind).bind(position as i64)
            .execute(&pool).await?;
    }
    // Drives can come and go; drop volume rows that no longer exist so already-removed drives
    // don't linger in the sidebar.
    let volume_paths: Vec<&str> = system
        .iter()
        .filter(|location| location.kind == "volume")
        .map(|location| location.path.as_str())
        .collect();
    if !volume_paths.is_empty() {
        let placeholders = std::iter::repeat_n("?", volume_paths.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql =
            format!("DELETE FROM locations WHERE kind = 'volume' AND path NOT IN ({placeholders})");
        let mut query = sqlx::query(&sql);
        for path in &volume_paths {
            query = query.bind(path);
        }
        query.execute(&pool).await?;
    }
    Ok(pool)
}

pub async fn apply_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
