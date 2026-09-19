//! Structured-data preview: `.json`, `.ndjson`/`.jsonl`, `.yaml`/`.yml` and
//! `.toml` are parsed into a `serde_json::Value` tree the frontend can fold.
//! Read-only; parsing is bounded by the shared source cap.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DataPreview {
    pub format: String,
    pub value: serde_json::Value,
    pub truncated: bool,
}

/// Most lines parsed from a JSON Lines file before the rest is ignored.
const MAX_NDJSON_LINES: usize = 5000;

pub(crate) fn is_data_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "json" | "ndjson" | "jsonl" | "yaml" | "yml" | "toml"
    )
}

#[tauri::command]
pub async fn open_data(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<DataPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    let extension = std::path::Path::new(&path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_data(&text, &extension)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn yaml_to_json(value: serde_yaml::Value) -> Result<serde_json::Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub(crate) fn parse_data(text: &str, extension: &str) -> Result<DataPreview, String> {
    match extension {
        "json" => {
            let value: serde_json::Value =
                serde_json::from_str(text).map_err(|error| format!("Invalid JSON: {error}"))?;
            Ok(DataPreview {
                format: "json".to_string(),
                value,
                truncated: false,
            })
        }
        "ndjson" | "jsonl" => {
            let mut values = Vec::new();
            let mut truncated = false;
            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if values.len() >= MAX_NDJSON_LINES {
                    truncated = true;
                    break;
                }
                let value: serde_json::Value = serde_json::from_str(line)
                    .map_err(|error| format!("Invalid JSON line: {error}"))?;
                values.push(value);
            }
            Ok(DataPreview {
                format: "ndjson".to_string(),
                value: serde_json::Value::Array(values),
                truncated,
            })
        }
        "yaml" | "yml" => {
            let value: serde_yaml::Value =
                serde_yaml::from_str(text).map_err(|error| format!("Invalid YAML: {error}"))?;
            Ok(DataPreview {
                format: "yaml".to_string(),
                value: yaml_to_json(value)?,
                truncated: false,
            })
        }
        "toml" => {
            let value: toml::Value =
                toml::from_str(text).map_err(|error| format!("Invalid TOML: {error}"))?;
            let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
            Ok(DataPreview {
                format: "toml".to_string(),
                value,
                truncated: false,
            })
        }
        other => Err(format!("Unsupported data format: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json() {
        let preview = parse_data(r#"{"a": 1, "b": [true, null]}"#, "json").unwrap();
        assert_eq!(preview.format, "json");
        assert_eq!(preview.value["a"], 1);
    }

    #[test]
    fn parses_ndjson() {
        let preview = parse_data("{\"a\":1}\n{\"a\":2}\n", "ndjson").unwrap();
        assert_eq!(preview.value.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parses_yaml() {
        let preview = parse_data("name: demo\nports:\n  - 80\n  - 443\n", "yaml").unwrap();
        assert_eq!(preview.value["name"], "demo");
        assert_eq!(preview.value["ports"][1], 443);
    }

    #[test]
    fn parses_toml() {
        let preview = parse_data("[server]\nport = 8080\n", "toml").unwrap();
        assert_eq!(preview.value["server"]["port"], 8080);
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_data("{nope", "json").is_err());
    }
}
