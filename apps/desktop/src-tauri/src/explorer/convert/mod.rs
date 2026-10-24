//! Image conversion: decode, transform, encode, write next to the original.

pub mod decode;
pub mod encode;
pub mod formats;
pub mod heif;
pub mod job;
pub mod metadata;
pub mod svg;
pub mod transform;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::app::error::AppError;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use formats::{ImageFormat, READABLE_EXTENSIONS};
use job::{ConvertOptions, Outcome};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub readable_extensions: Vec<&'static str>,
    pub writable: Vec<ImageFormat>,
}

/// What this build can open and write. The UI only offers these.
#[tauri::command]
pub fn convert_capabilities() -> Capabilities {
    Capabilities {
        readable_extensions: READABLE_EXTENSIONS.to_vec(),
        writable: ImageFormat::WRITABLE.to_vec(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    /// File size on disk.
    pub bytes: u64,
    pub has_gps: bool,
    pub has_alpha: bool,
    pub animated: bool,
}

/// Cheap facts the dialog shows and warns about; does not decode the pixels.
#[tauri::command]
#[tracing::instrument(skip_all, name = "convert_image_info", fields(sentry_op = "convert.info"))]
pub async fn convert_image_info(path: String) -> Result<ImageInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        let probe = decode::probe(&bytes)?;
        Ok::<_, String>(ImageInfo {
            width: probe.width,
            height: probe.height,
            bytes: bytes.len() as u64,
            has_gps: metadata::has_gps(&bytes)
                || decode::metadata_of(&bytes)
                    .exif
                    .is_some_and(|exif| metadata::exif_has_gps(&exif)),
            has_alpha: probe.has_alpha,
            animated: probe.animated,
        })
    })
    .await
    .map_err(crate::app::error::from_join)?
    .map_err(AppError::from)
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "estimate_convert", fields(sentry_op = "convert.estimate"))]
pub async fn estimate_convert(path: String, options: ConvertOptions) -> Result<u64, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        job::estimate_size(&path, &options)
    })
    .await
    .map_err(crate::app::error::from_join)?
    .map_err(AppError::from)
}

/// The converted image itself, at full resolution, for the before/after viewer.
#[tauri::command]
#[tracing::instrument(skip_all, name = "preview_convert", fields(sentry_op = "convert.preview"))]
pub async fn preview_convert(
    path: String,
    options: ConvertOptions,
) -> Result<tauri::ipc::Response, AppError> {
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        job::render_file(&path, &options)
    })
    .await
    .map_err(crate::app::error::from_join)?
    .map_err(AppError::from)?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Progress goes out as `transfer-progress` events; `cancel_transfer` stops it.
#[tauri::command]
#[tracing::instrument(skip_all, name = "convert_images", fields(sentry_op = "convert.run"))]
pub async fn convert_images(
    app: tauri::AppHandle,
    registry: tauri::State<'_, crate::remote::transfer::TransferRegistry>,
    job_id: String,
    paths: Vec<String>,
    options: ConvertOptions,
    replace_original: bool,
) -> Result<Outcome, AppError> {
    let token = registry.register(&job_id);
    let id = job_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = paths
            .iter()
            .map(|path| validate_existing(Path::new(path), ExpectedKind::File))
            .collect::<Result<_, _>>()?;
        let folder = paths
            .first()
            .and_then(|path| path.parent())
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut event = crate::remote::transfer::TransferEvent::new(id, "convert", folder, 0, 0);
        event.label = if paths.len() == 1 {
            paths[0]
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            format!("{} images", paths.len())
        };
        event.files_total = paths.len() as u64;
        crate::remote::transfer::emit(&app, &event);

        let outcome = job::run_batch(
            &paths,
            &options,
            replace_original,
            &|| token.is_cancelled(),
            &|path| trash::delete(path).map_err(|error| error.to_string()),
            &mut |done, total| {
                event.files_done = done;
                event.files_total = total;
                crate::remote::transfer::emit(&app, &event);
            },
        );
        event.state = if outcome.cancelled {
            "cancelled"
        } else if !outcome.failed.is_empty() && outcome.converted.is_empty() {
            "failed"
        } else {
            "done"
        }
        .to_string();
        event.error = outcome
            .failed
            .first()
            .map(|failure| format!("{}: {}", failure.name, failure.error));
        crate::remote::transfer::emit(&app, &event);
        Ok::<_, String>(outcome)
    })
    .await
    .map_err(crate::app::error::from_join);
    registry.remove(&job_id);
    result.and_then(|inner| inner.map_err(AppError::from))
}
