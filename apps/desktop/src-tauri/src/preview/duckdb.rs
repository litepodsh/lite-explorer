//! DuckDB database (`.duckdb`) preview: lists tables and shows a bounded grid
//! per table. Opens the file read-only and never writes. Every query runs on
//! one thread with a memory cap and a timeout, so a huge database can't starve
//! the explorer.

use std::time::Duration;

use duckdb::{types::Value, Connection, OptionalExt};
use tauri::State;

use crate::app::db::Database;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::preview::database::{ColumnInfo, DatabasePreview, TableData, TableInfo};

/// Rows shown per table before the grid is reported as truncated.
const MAX_ROWS: usize = 500;
/// Tables listed before the rest are ignored.
const MAX_TABLES: usize = 200;
/// Characters kept per cell; longer values are cut with an ellipsis.
const MAX_CELL_CHARS: usize = 1_000;
/// Wall time a preview query may take before it's interrupted.
const QUERY_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn is_duckdb_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("duckdb")
}

/// DuckDB files carry an 8-byte checksum before the `DUCK` magic, so sniff at
/// the header instead of the extension.
fn is_duckdb(path: &str) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 12];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map(|_| &magic[8..12] == b"DUCK")
        .unwrap_or(false)
}

fn connect(path: &str) -> Result<Connection, String> {
    let config = duckdb::Config::default()
        .access_mode(duckdb::AccessMode::ReadOnly)
        .and_then(|config| config.threads(1))
        .and_then(|config| config.max_memory("256MB"))
        .map_err(|error| error.to_string())?;
    Connection::open_with_flags(path, config).map_err(|error| error.to_string())
}

/// Runs `work` against a read-only connection on a blocking thread. Past
/// `QUERY_TIMEOUT` the running query is interrupted and an error is returned
/// right away, so the UI never waits on a slow database.
async fn with_connection<T, F>(path: String, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> Result<T, String> + Send + 'static,
{
    let (handle_tx, mut handle_rx) = tokio::sync::oneshot::channel();
    let task = tauri::async_runtime::spawn_blocking(move || {
        if !is_duckdb(&path) {
            return Err("Not a DuckDB database".to_string());
        }
        let conn = connect(&path)?;
        let _ = handle_tx.send(conn.interrupt_handle());
        work(&conn)
    });
    match tokio::time::timeout(QUERY_TIMEOUT, task).await {
        Ok(joined) => joined.map_err(|error| error.to_string())?,
        Err(_) => {
            if let Ok(handle) = handle_rx.try_recv() {
                handle.interrupt();
            }
            Err("The database took too long to respond".to_string())
        }
    }
}

fn clip(mut text: String) -> String {
    if let Some((index, _)) = text.char_indices().nth(MAX_CELL_CHARS) {
        text.truncate(index);
        text.push('…');
    }
    text
}

/// Rejects a table name that isn't a real table or view, so it can't be
/// interpolated into SQL as an injection.
fn known_table(conn: &Connection, table: &str) -> Result<String, String> {
    let exists: Option<String> = conn
        .query_row(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'main' AND table_name = ?",
            [table],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    exists.ok_or_else(|| format!("Unknown table: {table}"))
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Boolean(v) => v.to_string(),
        Value::TinyInt(v) => v.to_string(),
        Value::SmallInt(v) => v.to_string(),
        Value::Int(v) => v.to_string(),
        Value::BigInt(v) => v.to_string(),
        Value::HugeInt(v) => v.to_string(),
        Value::UHugeInt(v) => v.to_string(),
        Value::UTinyInt(v) => v.to_string(),
        Value::USmallInt(v) => v.to_string(),
        Value::UInt(v) => v.to_string(),
        Value::UBigInt(v) => v.to_string(),
        Value::Float(v) => v.to_string(),
        Value::Double(v) => v.to_string(),
        Value::Text(v) => v.clone(),
        Value::Blob(v) => format!("<{} bytes>", v.len()),
        Value::Geometry(v) => format!("<{} bytes WKB>", v.len()),
        Value::List(v) => format!("[{}]", v.iter().map(format_value).collect::<Vec<_>>().join(", ")),
        Value::Array(v) => format!("[{}]", v.iter().map(format_value).collect::<Vec<_>>().join(", ")),
        Value::Enum(v) => v.clone(),
        Value::Union(v) => format_value(v),
        // Decimal, Timestamp, Date32, Time64, Interval, Struct and Map: the raw
        // debug form is still readable enough for a bounded preview.
        other => format!("{other:?}"),
    }
}

#[tauri::command]
pub async fn open_duckdb(
    database: State<'_, Database>,
    path: String,
) -> Result<DatabasePreview, String> {
    let _ = &database;
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    list_tables(&path.to_string_lossy()).await
}

async fn list_tables(path: &str) -> Result<DatabasePreview, String> {
    with_connection(path.to_string(), |conn| {
        // `estimated_size` comes from table metadata, so listing never scans
        // data; views have no cheap count and report -1.
        let mut stmt = conn
            .prepare(
                "SELECT table_name, estimated_size FROM duckdb_tables()
                 WHERE schema_name = 'main' AND database_name = current_database()
                 UNION ALL
                 SELECT view_name, -1 FROM duckdb_views()
                 WHERE schema_name = 'main' AND database_name = current_database() AND NOT internal
                 ORDER BY 1 LIMIT ?",
            )
            .map_err(|error| error.to_string())?;
        let tables = stmt
            .query_map([MAX_TABLES as i64], |row| {
                Ok(TableInfo {
                    name: row.get(0)?,
                    rows: row.get(1)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(DatabasePreview { tables })
    })
    .await
}

#[tauri::command]
pub async fn read_duckdb_table(path: String, table: String) -> Result<TableData, String> {
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    read_table(&path.to_string_lossy(), &table).await
}

async fn read_table(path: &str, table: &str) -> Result<TableData, String> {
    let table = table.to_string();
    with_connection(path.to_string(), move |conn| {
        let name = known_table(conn, &table)?;

        let columns: Vec<ColumnInfo> = conn
            .prepare(&format!("PRAGMA table_info({})", quote(&name)))
            .map_err(|error| error.to_string())?
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get::<_, String>(1).unwrap_or_default(),
                    r#type: row.get::<_, String>(2).unwrap_or_default(),
                    primary_key: false,
                    not_null: false,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;

        let query = format!("SELECT * FROM {} LIMIT {}", quote(&name), MAX_ROWS + 1);
        let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
        let mut rows_iter = stmt.query([]).map_err(|error| error.to_string())?;
        // DuckDB only knows the result shape once the statement has run.
        let column_count = rows_iter.as_ref().map_or(0, |stmt| stmt.column_count());
        let mut rows = Vec::new();
        let mut truncated = false;
        while let Some(row) = rows_iter.next().map_err(|error| error.to_string())? {
            if rows.len() >= MAX_ROWS {
                truncated = true;
                break;
            }
            rows.push(
                (0..column_count)
                    .map(|index| {
                        let value: Value = row.get(index).unwrap_or(Value::Null);
                        clip(format_value(&value))
                    })
                    .collect(),
            );
        }

        Ok(TableData {
            name,
            columns,
            rows,
            truncated,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_extension() {
        assert!(is_duckdb_extension("duckdb"));
        assert!(is_duckdb_extension("DUCKDB"));
        assert!(!is_duckdb_extension("duckdb-wal"));
    }

    #[test]
    fn formats_values() {
        assert_eq!(format_value(&Value::Null), "NULL");
        assert_eq!(format_value(&Value::Text("hola".into())), "hola");
        assert_eq!(format_value(&Value::Int(42)), "42");
        assert_eq!(format_value(&Value::Double(18000.5)), "18000.5");
        assert_eq!(format_value(&Value::Blob(vec![1, 2, 3])), "<3 bytes>");
    }

    #[test]
    fn clips_long_cells() {
        assert_eq!(clip("corto".into()), "corto");
        let clipped = clip("ñ".repeat(MAX_CELL_CHARS + 5));
        assert_eq!(clipped.chars().count(), MAX_CELL_CHARS + 1);
        assert!(clipped.ends_with('…'));
    }

    #[tokio::test]
    async fn lists_and_reads_tables() {
        let path = std::env::temp_dir().join(format!("lite-duckdb-{}.duckdb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE productos(id INTEGER PRIMARY KEY, nombre VARCHAR, precio DOUBLE);
                 INSERT INTO productos VALUES (1, 'Café', 18000.5);",
            )
            .unwrap();
        }
        let path = path.to_str().unwrap();
        let preview = list_tables(path).await.unwrap();
        assert_eq!(preview.tables.len(), 1);
        assert_eq!(preview.tables[0].name, "productos");
        assert_eq!(preview.tables[0].rows, 1);

        let table = read_table(path, "productos").await.unwrap();
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.rows[0], vec!["1", "Café", "18000.5"]);
        std::fs::remove_file(path).unwrap();
    }
}
