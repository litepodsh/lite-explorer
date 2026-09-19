//! Photoshop document (`.psd`) preview: document metadata and the layer stack via
//! the `psd` crate. Read-only; layers are listed, not composited.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most layers listed from one document.
const MAX_LAYERS: usize = 2000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PsdLayer {
    pub name: String,
    pub kind: String,
    pub width: u16,
    pub height: u16,
    pub visible: bool,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PsdPreview {
    pub width: u32,
    pub height: u32,
    pub color_mode: String,
    pub layers: Vec<PsdLayer>,
    pub truncated: bool,
}

pub(crate) fn is_psd_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "psd" | "psb")
}

#[tauri::command]
pub async fn open_psd(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<PsdPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_psd(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

pub(crate) fn parse_psd(bytes: &[u8]) -> Result<PsdPreview, String> {
    let psd = psd::Psd::from_bytes(bytes).map_err(|error| format!("Invalid PSD: {error}"))?;
    let mut preview = PsdPreview {
        width: psd.width(),
        height: psd.height(),
        color_mode: format!("{:?}", psd.color_mode()),
        ..Default::default()
    };
    for layer in psd.layers() {
        if preview.layers.len() >= MAX_LAYERS {
            preview.truncated = true;
            break;
        }
        preview.layers.push(PsdLayer {
            name: layer.name().to_string(),
            kind: format!("{:?}", layer.blend_mode()),
            width: layer.width(),
            height: layer.height(),
            visible: layer.visible(),
        });
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert!(parse_psd(b"not a psd").is_err());
    }
}
