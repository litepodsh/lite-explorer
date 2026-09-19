//! Sketch document (`.sketch`) preview: a zip of JSON. Pages and their top-level
//! layers are listed and drawn as an offline SVG wireframe. Read-only.

use std::{fs::File, io::BufReader};

use serde::Serialize;
use zip::ZipArchive;

use crate::preview::source::{read_local_bytes, SOURCE_MAX_BYTES};

/// Most layers collected from one page.
const MAX_LAYERS: usize = 2000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SketchLayer {
    pub name: String,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub visible: bool,
    pub text: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SketchPage {
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<SketchLayer>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SketchPreview {
    pub pages: Vec<SketchPage>,
}

pub(crate) fn is_sketch_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("sketch")
}

#[tauri::command]
pub async fn open_sketch(path: String) -> Result<SketchPreview, String> {
    let _ = read_local_bytes(&path, SOURCE_MAX_BYTES)?;
    tauri::async_runtime::spawn_blocking(move || parse_sketch(&path))
        .await
        .map_err(|error| error.to_string())?
}

fn read_zip_text(archive: &mut ZipArchive<BufReader<File>>, name: &str) -> Option<String> {
    let mut file = archive.by_name(name).ok()?;
    let mut text = String::new();
    use std::io::Read;
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

fn layer_of(value: &serde_json::Value) -> SketchLayer {
    let frame = value.get("frame");
    let number = |key: &str| {
        frame
            .and_then(|frame| frame.get(key))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    let text = value
        .get("attributedString")
        .and_then(|string| string.get("string"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("name").and_then(serde_json::Value::as_str))
        .unwrap_or("")
        .to_string();
    SketchLayer {
        name: value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        kind: value
            .get("_class")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("layer")
            .to_string(),
        x: number("x"),
        y: number("y"),
        width: number("width"),
        height: number("height"),
        visible: value
            .get("isVisible")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        text,
    }
}

pub(crate) fn parse_sketch(path: &str) -> Result<SketchPreview, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|error| error.to_string())?;

    let document: serde_json::Value = serde_json::from_str(
        &read_zip_text(&mut archive, "document.json").ok_or("Missing document.json")?,
    )
    .map_err(|error| error.to_string())?;

    let page_refs = document
        .get("pages")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut preview = SketchPreview::default();
    for page_ref in page_refs {
        let id = page_ref
            .get("_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| page_ref.get("_ref").and_then(serde_json::Value::as_str))
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        let name = page_ref
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Page")
            .to_string();
        let page_json = read_zip_text(&mut archive, &format!("pages/{id}.json"));
        let mut page = SketchPage {
            name,
            ..Default::default()
        };
        if let Some(text) = page_json {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(page_name) = value.get("name").and_then(serde_json::Value::as_str) {
                    page.name = page_name.to_string();
                }
                collect_layers(value.get("layers"), &mut page.layers);
            }
        }
        // Page bounds are the union of its layers (or its artboards).
        for layer in &page.layers {
            page.width = page.width.max(layer.x + layer.width);
            page.height = page.height.max(layer.y + layer.height);
        }
        preview.pages.push(page);
    }

    if preview.pages.is_empty() {
        return Err("No pages found in document".to_string());
    }
    Ok(preview)
}

fn collect_layers(layers: Option<&serde_json::Value>, out: &mut Vec<SketchLayer>) {
    let Some(layers) = layers.and_then(serde_json::Value::as_array) else {
        return;
    };
    for layer in layers {
        if out.len() >= MAX_LAYERS {
            return;
        }
        let class = layer
            .get("_class")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        // Groups have no frame of their own; descend into them.
        if class == "group" || class == "shapeGroup" {
            collect_layers(layer.get("layers"), out);
            continue;
        }
        let parsed = layer_of(layer);
        if parsed.width > 0.0 || parsed.height > 0.0 {
            out.push(parsed);
        }
        // Artboards contain the shapes; keep the frame and descend.
        if class == "artboard" {
            collect_layers(layer.get("layers"), out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_zip() {
        let path = std::env::temp_dir().join("lite-sketch-bad.sketch");
        std::fs::write(&path, b"not a zip").unwrap();
        assert!(parse_sketch(path.to_str().unwrap()).is_err());
        std::fs::remove_file(&path).unwrap();
    }
}
