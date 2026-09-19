//! Apache Avro (`.avro`) preview: the writer schema plus the first records as a
//! grid. Read-only.

use apache_avro::{types::Value, Reader};
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most records shown from one file.
const MAX_ROWS: usize = 500;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AvroPreview {
    pub schema: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

pub(crate) fn is_avro_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("avro")
}

#[tauri::command]
pub async fn open_avro(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<AvroPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_avro(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn cell(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `apache_avro::Value` has no `Serialize` impl, so map it to JSON by hand.
fn to_json(value: &Value) -> serde_json::Value {
    use serde_json::Value as Json;
    match value {
        Value::Null => Json::Null,
        Value::Boolean(flag) => Json::Bool(*flag),
        Value::Int(number) => Json::from(*number),
        Value::Long(number) => Json::from(*number),
        Value::Float(number) => Json::from(*number as f64),
        Value::Double(number) => Json::from(*number),
        Value::Date(days) => Json::from(*days),
        Value::TimeMillis(millis) => Json::from(*millis),
        Value::TimeMicros(micros) => Json::from(*micros),
        Value::TimestampMillis(millis) => Json::from(*millis),
        Value::TimestampMicros(micros) => Json::from(*micros),
        Value::LocalTimestampMillis(millis) => Json::from(*millis),
        Value::LocalTimestampMicros(micros) => Json::from(*micros),
        Value::String(text) => Json::String(text.clone()),
        Value::Enum(_, label) => Json::String(label.clone()),
        Value::Bytes(bytes) | Value::Fixed(_, bytes) => {
            Json::String(format!("<{} bytes>", bytes.len()))
        }
        Value::Union(_, inner) => to_json(inner),
        Value::Array(items) => Json::Array(items.iter().map(to_json).collect()),
        Value::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), to_json(item)))
                .collect(),
        ),
        Value::Record(fields) => Json::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), to_json(item)))
                .collect(),
        ),
        other => Json::String(format!("{other:?}")),
    }
}

pub(crate) fn parse_avro(bytes: &[u8]) -> Result<AvroPreview, String> {
    let reader = Reader::new(bytes).map_err(|error| format!("Invalid Avro file: {error}"))?;
    let schema = serde_json::to_string(reader.writer_schema()).unwrap_or_default();

    let mut columns: Vec<String> = Vec::new();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut truncated = false;

    for record in reader {
        let record = record.map_err(|error| error.to_string())?;
        let json = to_json(&record);
        match &json {
            serde_json::Value::Object(map) => {
                if columns.is_empty() {
                    columns = map.keys().cloned().collect();
                }
                rows.push(
                    columns
                        .iter()
                        .map(|key| map.get(key).map(cell).unwrap_or_default())
                        .collect(),
                );
            }
            other => {
                if columns.is_empty() {
                    columns.push("value".to_string());
                }
                rows.push(vec![cell(other)]);
            }
        }
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
    }

    if rows.is_empty() {
        return Err("No records found".to_string());
    }
    Ok(AvroPreview {
        schema,
        columns,
        rows,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use apache_avro::{types::Value, Schema, Writer};

    #[test]
    fn parses_schema_and_records() {
        let schema = Schema::parse_str(r#"{"type":"record","name":"Weather","fields":[{"name":"city","type":"string"},{"name":"temp","type":"int"}]}"#).unwrap();
        let record = Value::Record(vec![
            ("city".into(), Value::String("Bogotá".into())),
            ("temp".into(), Value::Int(18)),
        ]);
        let mut writer = Writer::new(&schema, Vec::new());
        writer.append(record).unwrap();
        let bytes = writer.into_inner().unwrap();
        let preview = parse_avro(&bytes).unwrap();
        assert_eq!(preview.columns, vec!["city", "temp"]);
        assert_eq!(preview.rows[0], vec!["Bogotá", "18"]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_avro(b"not avro").is_err());
    }
}
