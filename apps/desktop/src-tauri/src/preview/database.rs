//! SQLite database (`.sqlite`, `.db`, `.sqlite3`) preview: lists tables and shows
//! a bounded grid per table. Opens the file read-only and never writes.

use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
    Row, TypeInfo, ValueRef,
};
use tauri::State;

use crate::app::db::Database;

/// Rows shown per table before the grid is reported as truncated.
const MAX_ROWS: usize = 500;
/// Tables listed before the rest are ignored.
const MAX_TABLES: usize = 200;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    pub rows: i64,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    pub r#type: String,
    pub primary_key: bool,
    pub not_null: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TableData {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePreview {
    pub tables: Vec<TableInfo>,
}

pub(crate) fn is_database_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "sqlite" | "sqlite3" | "db"
    )
}

fn is_sqlite(path: &str) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map(|_| magic == *b"SQLite format 3\0")
        .unwrap_or(false)
}

async fn connect(path: &str) -> Result<sqlx::SqlitePool, String> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| error.to_string())
}

/// Rejects a table name that isn't a real table, so it can't be interpolated
/// into SQL as an injection.
async fn known_table(pool: &sqlx::SqlitePool, table: &str) -> Result<String, String> {
    let exists: Option<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name = ?")
            .bind(table)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    exists.ok_or_else(|| format!("Unknown table: {table}"))
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn format_cell(row: &SqliteRow, index: usize) -> String {
    let raw = match row.try_get_raw(index) {
        Ok(raw) => raw,
        Err(_) => return String::new(),
    };
    if raw.is_null() {
        return "NULL".to_string();
    }
    let kind = raw.type_info().name().to_ascii_uppercase();
    if kind.contains("INT") {
        if let Ok(value) = row.try_get::<i64, _>(index) {
            return value.to_string();
        }
    }
    if kind.contains("REAL") || kind.contains("FLOA") || kind.contains("DOUB") {
        if let Ok(value) = row.try_get::<f64, _>(index) {
            return value.to_string();
        }
    }
    if kind.contains("BLOB") {
        if let Ok(value) = row.try_get::<Vec<u8>, _>(index) {
            return format!("<{} bytes>", value.len());
        }
    }
    if let Ok(value) = row.try_get::<String, _>(index) {
        return value;
    }
    if let Ok(value) = row.try_get::<f64, _>(index) {
        return value.to_string();
    }
    if let Ok(value) = row.try_get::<i64, _>(index) {
        return value.to_string();
    }
    String::new()
}

#[tauri::command]
pub async fn open_database(
    database: State<'_, Database>,
    path: String,
) -> Result<DatabasePreview, String> {
    let _ = &database;
    list_tables(&path).await
}

async fn list_tables(path: &str) -> Result<DatabasePreview, String> {
    if !is_sqlite(path) {
        return Err("Not a SQLite database".to_string());
    }
    let pool = connect(path).await?;
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut tables = Vec::new();
    for name in names.into_iter().take(MAX_TABLES) {
        let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {}", quote(&name)))
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
        tables.push(TableInfo { name, rows });
    }
    pool.close().await;
    Ok(DatabasePreview { tables })
}

#[tauri::command]
pub async fn read_sqlite_table(path: String, table: String) -> Result<TableData, String> {
    read_table(&path, &table).await
}

async fn read_table(path: &str, table: &str) -> Result<TableData, String> {
    if !is_sqlite(path) {
        return Err("Not a SQLite database".to_string());
    }
    let pool = connect(path).await?;
    let name = known_table(&pool, table).await?;

    let info = sqlx::query(&format!("PRAGMA table_info({})", quote(&name)))
        .fetch_all(&pool)
        .await
        .map_err(|error| error.to_string())?;
    let columns = info
        .iter()
        .map(|row| ColumnInfo {
            name: row.try_get::<String, _>("name").unwrap_or_default(),
            r#type: row.try_get::<String, _>("type").unwrap_or_default(),
            primary_key: row.try_get::<i64, _>("pk").unwrap_or(0) > 0,
            not_null: row.try_get::<i64, _>("notnull").unwrap_or(0) > 0,
        })
        .collect::<Vec<_>>();

    let query = format!("SELECT * FROM {} LIMIT {}", quote(&name), MAX_ROWS + 1);
    let raw_rows = sqlx::query(&query)
        .fetch_all(&pool)
        .await
        .map_err(|error| error.to_string())?;
    pool.close().await;

    let truncated = raw_rows.len() > MAX_ROWS;
    let rows = raw_rows
        .iter()
        .take(MAX_ROWS)
        .map(|row| {
            (0..row.len())
                .map(|index| format_cell(row, index))
                .collect()
        })
        .collect();

    Ok(TableData {
        name,
        columns,
        rows,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn detects_extension() {
        assert!(is_database_extension("DB"));
        assert!(is_database_extension("sqlite3"));
        assert!(!is_database_extension("sqlite-wal"));
    }

    #[test]
    fn checks_the_sqlite_magic() {
        let path = std::env::temp_dir().join("lite-db-magic.sqlite");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"SQLite format 3\0rest").unwrap();
        drop(file);
        assert!(is_sqlite(path.to_str().unwrap()));
        std::fs::remove_file(&path).unwrap();
    }

    #[tokio::test]
    async fn lists_and_reads_tables() {
        let path = std::env::temp_dir().join(format!("lite-db-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE productos(id INTEGER PRIMARY KEY, nombre TEXT, precio REAL)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO productos VALUES (1, 'Café', 18000.5)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let path = path.to_str().unwrap();
        let preview = list_tables(path).await.unwrap();
        assert_eq!(preview.tables.len(), 1);
        assert_eq!(preview.tables[0].name, "productos");
        assert_eq!(preview.tables[0].rows, 1);

        let table = read_table(path, "productos").await.unwrap();
        assert_eq!(table.columns.len(), 3);
        assert!(table.columns[0].primary_key);
        assert_eq!(table.rows[0], vec!["1", "Café", "18000.5"]);
        std::fs::remove_file(path).unwrap();
    }
}
