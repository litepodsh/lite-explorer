//! Apache Arrow IPC (`.arrow`, `.feather`) preview: the schema plus the first
//! records as a grid, read with the `arrow` IPC readers. Read-only.

use std::io::Cursor;

use arrow::{
    datatypes::SchemaRef,
    ipc::reader::{FileReader, StreamReader},
    record_batch::RecordBatch,
    util::display::array_value_to_string,
};
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most records shown from one file.
const MAX_ROWS: usize = 500;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArrowPreview {
    pub schema: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

pub(crate) fn is_arrow_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "arrow" | "feather" | "ipc"
    )
}

#[tauri::command]
pub async fn open_arrow(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<ArrowPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_arrow(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn read_batches(bytes: &[u8]) -> Result<(SchemaRef, Vec<RecordBatch>), String> {
    // IPC files start with the `ARROW1` magic; streams don't, so fall back to that.
    if bytes.starts_with(b"ARROW1") {
        let reader = FileReader::try_new(Cursor::new(bytes), None)
            .map_err(|error| format!("Invalid Arrow file: {error}"))?;
        let schema = reader.schema();
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok((schema, batches))
    } else {
        let reader = StreamReader::try_new(Cursor::new(bytes), None)
            .map_err(|error| format!("Invalid Arrow stream: {error}"))?;
        let schema = reader.schema();
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok((schema, batches))
    }
}

pub(crate) fn parse_arrow(bytes: &[u8]) -> Result<ArrowPreview, String> {
    let (schema, batches) = read_batches(bytes)?;
    if schema.fields().is_empty() {
        return Err("No schema found".to_string());
    }

    let columns: Vec<String> = schema
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect();
    let schema_text = schema
        .fields()
        .iter()
        .map(|field| format!("{}: {}", field.name(), field.data_type()))
        .collect::<Vec<_>>()
        .join("\n");

    let mut rows = Vec::new();
    let mut truncated = false;
    'outer: for batch in &batches {
        for index in 0..batch.num_rows() {
            rows.push(
                batch
                    .columns()
                    .iter()
                    .map(|column| array_value_to_string(column, index).unwrap_or_default())
                    .collect(),
            );
            if rows.len() >= MAX_ROWS {
                truncated = true;
                break 'outer;
            }
        }
    }

    Ok(ArrowPreview {
        schema: schema_text,
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
        assert!(parse_arrow(b"not arrow").is_err());
    }
}
