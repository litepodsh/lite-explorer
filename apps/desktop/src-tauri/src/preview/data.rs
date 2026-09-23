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
        "json" | "jsonc" | "ndjson" | "jsonl" | "yaml" | "yml" | "toml"
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

/// Strips `//`/`/* */` comments and trailing commas so JSONC parses as JSON.
/// Only ASCII structural bytes are removed, so UTF-8 content stays intact.
fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        let byte = bytes[i];
        if in_string {
            out.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match byte {
            b'"' => {
                in_string = true;
                out.push(byte);
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(bytes.len());
            }
            b',' => {
                let mut next = i + 1;
                while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                    next += 1;
                }
                if next < bytes.len() && (bytes[next] == b'}' || bytes[next] == b']') {
                    i += 1;
                } else {
                    out.push(byte);
                    i += 1;
                }
            }
            _ => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
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
        "jsonc" => {
            let value: serde_json::Value = serde_json::from_str(&strip_jsonc(text))
                .map_err(|error| format!("Invalid JSON: {error}"))?;
            Ok(DataPreview {
                format: "jsonc".to_string(),
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
    fn parses_jsonc_with_comments_and_trailing_commas() {
        let preview = parse_data(
            "{\n  // line comment\n  \"a\": 1,\n  /* block */ \"b\": [true, false,],\n}\n",
            "jsonc",
        )
        .unwrap();
        assert_eq!(preview.format, "jsonc");
        assert_eq!(preview.value["a"], 1);
        assert_eq!(preview.value["b"][0], true);
    }

    #[test]
    fn keeps_comment_like_text_inside_strings() {
        let preview = parse_data(r#"{"url": "https://example.com//x"}"#, "jsonc").unwrap();
        assert_eq!(preview.value["url"], "https://example.com//x");
    }

    #[test]
    fn recognizes_jsonc_as_data() {
        assert!(is_data_extension("jsonc"));
        assert!(is_data_extension("JSONC"));
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
