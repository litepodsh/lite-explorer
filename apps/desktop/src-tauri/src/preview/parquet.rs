//! Apache Parquet (`.parquet`) preview: the schema plus the first records as a
//! grid, using the low-level record reader (no Arrow). Read-only.

use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most records shown from one file.
const MAX_ROWS: usize = 500;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ParquetPreview {
    pub schema: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

pub(crate) fn is_parquet_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("parquet")
}

#[tauri::command]
pub async fn open_parquet(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<ParquetPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_parquet(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

pub(crate) fn parse_parquet(bytes: &[u8]) -> Result<ParquetPreview, String> {
    if !bytes.starts_with(b"PAR1") {
        return Err("Not a Parquet file".to_string());
    }
    let reader = SerializedFileReader::new(Bytes::from(bytes.to_vec()))
        .map_err(|error| format!("Invalid Parquet file: {error}"))?;

    let metadata = reader.metadata();
    let mut schema_bytes = Vec::new();
    parquet::schema::printer::print_schema(
        &mut schema_bytes,
        metadata.file_metadata().schema_descr().root_schema(),
    );
    let schema = String::from_utf8_lossy(&schema_bytes).into_owned();
    let columns: Vec<String> = metadata
        .file_metadata()
        .schema_descr()
        .columns()
        .iter()
        .map(|column| column.name().to_string())
        .collect();

    let mut rows = Vec::new();
    let mut truncated = false;
    let iterator = reader
        .get_row_iter(None)
        .map_err(|error| error.to_string())?;
    for row in iterator {
        let row = row.map_err(|error| error.to_string())?;
        rows.push(
            row.get_column_iter()
                .map(|(_, field)| field.to_string())
                .collect(),
        );
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
    }

    if rows.is_empty() {
        return Err("No records found".to_string());
    }
    Ok(ParquetPreview {
        schema,
        columns,
        rows,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(parse_parquet(b"not parquet").is_err());
        assert!(parse_parquet(b"PAR1garbage").is_err());
    }
}
