//! Jupyter notebook (`.ipynb`) preview: parses the JSON into markdown/code cells
//! with their textual, HTML and image outputs. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most cells rendered from one notebook.
const MAX_CELLS: usize = 1000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotebookOutput {
    /// `stream`, `error`, `html`, `image` or `text`.
    pub kind: String,
    pub mime: String,
    pub text: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotebookCell {
    /// `markdown`, `code` or `raw`.
    pub kind: String,
    pub source: String,
    pub outputs: Vec<NotebookOutput>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotebookPreview {
    pub language: String,
    pub kernel: String,
    pub cells: Vec<NotebookCell>,
    pub truncated: bool,
}

pub(crate) fn is_notebook_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("ipynb")
}

#[tauri::command]
pub async fn open_notebook(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<NotebookPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_notebook(&text)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn join_source(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(lines) => lines
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<String>(),
        _ => String::new(),
    }
}

fn pick_output(data: &serde_json::Value) -> Option<NotebookOutput> {
    if let Some(html) = data.get("text/html").filter(|value| !value.is_null()) {
        return Some(NotebookOutput {
            kind: "html".to_string(),
            mime: "text/html".to_string(),
            text: join_source(html),
        });
    }
    if let Some(image) = data.get("image/png").filter(|value| !value.is_null()) {
        let encoded: String = join_source(image).split_whitespace().collect();
        return Some(NotebookOutput {
            kind: "image".to_string(),
            mime: "image/png".to_string(),
            text: format!("data:image/png;base64,{encoded}"),
        });
    }
    if let Some(image) = data.get("image/jpeg").filter(|value| !value.is_null()) {
        let encoded: String = join_source(image).split_whitespace().collect();
        return Some(NotebookOutput {
            kind: "image".to_string(),
            mime: "image/jpeg".to_string(),
            text: format!("data:image/jpeg;base64,{encoded}"),
        });
    }
    data.get("text/plain")
        .filter(|value| !value.is_null())
        .map(|plain| NotebookOutput {
            kind: "text".to_string(),
            mime: "text/plain".to_string(),
            text: join_source(plain),
        })
}

fn cell_outputs(cell: &serde_json::Value) -> Vec<NotebookOutput> {
    let Some(outputs) = cell.get("outputs").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for output in outputs {
        match output
            .get("output_type")
            .and_then(serde_json::Value::as_str)
        {
            Some("stream") => result.push(NotebookOutput {
                kind: "stream".to_string(),
                mime: "text/plain".to_string(),
                text: join_source(&output["text"]),
            }),
            Some("error") => {
                let ename = output
                    .get("ename")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let evalue = output
                    .get("evalue")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let traceback = output
                    .get("traceback")
                    .and_then(serde_json::Value::as_array)
                    .map(|lines| {
                        lines
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();
                result.push(NotebookOutput {
                    kind: "error".to_string(),
                    mime: "text/plain".to_string(),
                    text: format!("{ename}: {evalue}\n{traceback}").trim().to_string(),
                });
            }
            Some("execute_result") | Some("display_data") => {
                if let Some(picked) = output.get("data").and_then(pick_output) {
                    result.push(picked);
                }
            }
            _ => {}
        }
    }
    result
}

pub(crate) fn parse_notebook(text: &str) -> Result<NotebookPreview, String> {
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("Invalid notebook: {error}"))?;
    let metadata = root.get("metadata").cloned().unwrap_or_default();
    let language = metadata
        .get("language_info")
        .and_then(|info| info.get("name"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .get("kernelspec")
                .and_then(|spec| spec.get("language"))
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    let kernel = metadata
        .get("kernelspec")
        .and_then(|spec| spec.get("display_name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string();

    let mut preview = NotebookPreview {
        language,
        kernel,
        ..Default::default()
    };
    let Some(cells) = root.get("cells").and_then(serde_json::Value::as_array) else {
        return Err("Notebook has no cells".to_string());
    };
    for cell in cells {
        if preview.cells.len() >= MAX_CELLS {
            preview.truncated = true;
            break;
        }
        let kind = cell
            .get("cell_type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("code")
            .to_string();
        preview.cells.push(NotebookCell {
            kind,
            source: join_source(&cell["source"]),
            outputs: cell_outputs(cell),
        });
    }
    if preview.cells.is_empty() {
        return Err("Notebook has no cells".to_string());
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cells_and_outputs() {
        let text = r##"{"metadata":{"kernelspec":{"display_name":"Python 3","language":"python"}},
            "cells":[
              {"cell_type":"markdown","source":["# Título\n","Texto"]},
              {"cell_type":"code","source":["print(1)"],
               "outputs":[{"output_type":"stream","text":["1\n"]},
                          {"output_type":"display_data","data":{"image/png":"aGk=","text/plain":"<img>"}}]}]}"##;
        let preview = parse_notebook(text).unwrap();
        assert_eq!(preview.language, "python");
        assert_eq!(preview.kernel, "Python 3");
        assert_eq!(preview.cells.len(), 2);
        assert_eq!(preview.cells[0].kind, "markdown");
        assert_eq!(preview.cells[0].source, "# Título\nTexto");
        let outputs = &preview.cells[1].outputs;
        assert_eq!(outputs[0].kind, "stream");
        assert_eq!(outputs[0].text, "1\n");
        assert_eq!(outputs[1].kind, "image");
        assert_eq!(outputs[1].text, "data:image/png;base64,aGk=");
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_notebook("{nope").is_err());
    }
}
