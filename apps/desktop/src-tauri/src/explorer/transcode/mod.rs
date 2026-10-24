//! Video and audio conversion with ffmpeg: finding it, downloading it, and reading files with ffprobe.

pub mod estimate;
pub mod frame;
pub mod install;
pub mod manifest;
pub mod options;
pub mod probe;
pub mod progress;
pub mod run;
pub mod tools;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app::error::AppError;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::explorer::convert::job::Outcome;
use options::TranscodeOptions;
use probe::MediaInfo;
use tools::{Source, Tools};

/// The folder the app installs its own ffmpeg into.
fn managed_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("ffmpeg").join(manifest::VERSION))
}

fn find_tools(app: &AppHandle) -> Option<Tools> {
    tools::locate(managed_dir(app).as_deref(), &tools::system_dirs())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FfmpegStatus {
    pub available: bool,
    pub source: Option<Source>,
    pub version: Option<String>,
    /// Which of the encoders the converter uses this build has.
    pub encoders: Vec<String>,
    /// Whether the app has a build to download for this system.
    pub can_download: bool,
    pub download_size: Option<u64>,
    pub download_version: &'static str,
    /// The graphics encoder that works on this machine ("videotoolbox" or "nvenc").
    pub hardware: Option<&'static str>,
}

fn status_of(app: &AppHandle) -> FfmpegStatus {
    let found = find_tools(app);
    let items = manifest::items();
    FfmpegStatus {
        available: found.is_some(),
        source: found.as_ref().map(|tools| tools.source),
        version: found.as_ref().map(|tools| tools.version.clone()),
        encoders: found.as_ref().map(tools::encoders).unwrap_or_default(),
        can_download: items.is_some(),
        download_size: items.map(manifest::total_size),
        download_version: manifest::VERSION,
        hardware: found.as_ref().and_then(tools::detect_hardware).map(|encoder| match encoder {
            options::HardwareEncoder::VideoToolbox => "videotoolbox",
            options::HardwareEncoder::Nvenc => "nvenc",
        }),
    }
}

/// Whether ffmpeg is available, which build it is, and what it can encode.
#[tauri::command]
#[tracing::instrument(skip_all, name = "ffmpeg_status", fields(sentry_op = "ffmpeg.status"))]
pub async fn ffmpeg_status(app: AppHandle) -> Result<FfmpegStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || status_of(&app))
        .await
        .map_err(crate::app::error::from_join)
}

/// Downloads the pinned ffmpeg into the app's folder. Progress goes out as `transfer-progress`
/// events and `cancel_transfer` stops it.
#[tauri::command]
#[tracing::instrument(skip_all, name = "ffmpeg_download", fields(sentry_op = "ffmpeg.download"))]
pub async fn ffmpeg_download(
    app: AppHandle,
    registry: tauri::State<'_, crate::remote::transfer::TransferRegistry>,
    job_id: String,
) -> Result<FfmpegStatus, AppError> {
    let items = manifest::items().ok_or(
        "Automatic download isn’t available on this system. Install ffmpeg and try again.",
    )?;
    let dir = managed_dir(&app).ok_or("The app data folder is not available")?;
    let token = registry.register(&job_id);
    let mut event = crate::remote::transfer::TransferEvent::new(
        job_id.clone(),
        "download",
        dir.to_string_lossy().into_owned(),
        1,
        manifest::total_size(items),
    );
    event.label = "Downloading ffmpeg".into();
    crate::remote::transfer::emit(&app, &event);

    let client = reqwest::Client::builder()
        .user_agent(concat!("lite-explorer/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let mut last_emit: Option<Instant> = None;
    let result = install::install(
        &dir,
        items,
        &client,
        &mut |done, _| {
            if last_emit.is_some_and(|at| at.elapsed() < Duration::from_millis(100)) {
                return;
            }
            last_emit = Some(Instant::now());
            event.bytes_done = done;
            crate::remote::transfer::emit(&app, &event);
        },
        &|| token.is_cancelled(),
    )
    .await;
    registry.remove(&job_id);

    match result {
        Ok(()) => {
            event.bytes_done = event.bytes_total;
            event.files_done = 1;
            event.state = "done".into();
            crate::remote::transfer::emit(&app, &event);
            Ok(status_of(&app))
        }
        Err(message) => {
            event.state = if message == install::CANCELLED { "cancelled" } else { "failed" }.into();
            if message != install::CANCELLED {
                event.error = Some(message.clone());
            }
            crate::remote::transfer::emit(&app, &event);
            Err(AppError::from(message))
        }
    }
}

/// Duration and streams of a video or audio file.
#[tauri::command]
#[tracing::instrument(skip_all, name = "media_probe", fields(sentry_op = "ffmpeg.probe"))]
pub async fn media_probe(app: AppHandle, path: String) -> Result<MediaInfo, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        let tools = find_tools(&app).ok_or("ffmpeg isn’t installed")?;
        probe::probe(&tools, &path)
    })
    .await
    .map_err(crate::app::error::from_join)?
    .map_err(AppError::from)
}

/// About how many bytes converting `path` with `options` would produce, from a few short samples.
/// `cancel_transfer` with the same job id stops it.
#[tauri::command]
#[tracing::instrument(skip_all, name = "transcode_estimate", fields(sentry_op = "ffmpeg.estimate"))]
pub async fn transcode_estimate(
    app: AppHandle,
    registry: tauri::State<'_, crate::remote::transfer::TransferRegistry>,
    job_id: String,
    path: String,
    options: TranscodeOptions,
) -> Result<u64, AppError> {
    let token = registry.register(&job_id);
    let result = tauri::async_runtime::spawn_blocking(move || {
        options::validate(&options)?;
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        let tools = find_tools(&app).ok_or("ffmpeg isn’t installed")?;
        let hardware = if options.wants_hardware() { tools::detect_hardware(&tools) } else { None };
        estimate::estimate(&tools, &path, &options, hardware, &|| token.is_cancelled())
    })
    .await
    .map_err(crate::app::error::from_join);
    registry.remove(&job_id);
    result.and_then(|inner| inner.map_err(AppError::from))
}

/// A picture of the video at `time` seconds, as it is (`converted` false) or as it would come out
/// of the conversion (`converted` true). Both have the same size, ready to compare.
#[tauri::command]
#[tracing::instrument(skip_all, name = "transcode_frame", fields(sentry_op = "ffmpeg.frame"))]
pub async fn transcode_frame(
    app: AppHandle,
    path: String,
    options: TranscodeOptions,
    time: f64,
    converted: bool,
) -> Result<tauri::ipc::Response, AppError> {
    let bytes = tauri::async_runtime::spawn_blocking(move || {
        options::validate(&options)?;
        if !time.is_finite() {
            return Err("The time is not a number".to_string());
        }
        let path = validate_existing(Path::new(&path), ExpectedKind::File)?;
        let tools = find_tools(&app).ok_or("ffmpeg isn’t installed")?;
        if converted {
            let hardware = if options.wants_hardware() { tools::detect_hardware(&tools) } else { None };
            frame::converted_frame(&tools, &path, &options, hardware, time, &|| false)
        } else {
            frame::original_frame(&tools, &path, time, &|| false)
        }
    })
    .await
    .map_err(crate::app::error::from_join)?
    .map_err(AppError::from)?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// A suffix becomes part of a file name, so it cannot hold a path separator.
fn validate_suffix(suffix: &str) -> Result<(), String> {
    if suffix.chars().any(|c| c == '/' || c == '\\' || c.is_control()) {
        return Err("The name suffix can’t contain slashes or control characters".into());
    }
    Ok(())
}

/// What the job is called in Activity.
fn job_label(options: &TranscodeOptions, count: usize) -> String {
    let what = match options {
        TranscodeOptions::Video { .. } => "video",
        TranscodeOptions::Audio { .. } => "audio",
        TranscodeOptions::Animation { .. } => "clip",
    };
    if count == 1 {
        format!("Convert {what}")
    } else {
        format!("Convert {count} {what}s")
    }
}

/// Converts each file with ffmpeg, next to the original. Progress goes out as
/// `transfer-progress` events and `cancel_transfer` stops it.
#[tauri::command]
#[tracing::instrument(skip_all, name = "transcode_run", fields(sentry_op = "ffmpeg.run"))]
pub async fn transcode_run(
    app: AppHandle,
    registry: tauri::State<'_, crate::remote::transfer::TransferRegistry>,
    job_id: String,
    paths: Vec<String>,
    options: TranscodeOptions,
    suffix: Option<String>,
    replace_original: bool,
) -> Result<Outcome, AppError> {
    let token = registry.register(&job_id);
    let id = job_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        options::validate(&options)?;
        let suffix = suffix.unwrap_or_default();
        validate_suffix(&suffix)?;
        let tools = find_tools(&app).ok_or("ffmpeg isn’t installed")?;
        let available = tools::encoders(&tools);
        if let Some(missing) = options
            .required_encoders()
            .into_iter()
            .find(|name| !available.iter().any(|have| have == name))
        {
            return Err(format!(
                "This ffmpeg build can’t create .{} files ({missing} is missing)",
                options.extension()
            ));
        }
        let paths: Vec<PathBuf> = paths
            .iter()
            .map(|path| validate_existing(Path::new(path), ExpectedKind::File))
            .collect::<Result<_, _>>()?;
        let folder = paths
            .first()
            .and_then(|path| path.parent())
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Progress is counted in thousandths of a file, so the bar moves smoothly inside each one.
        let mut event = crate::remote::transfer::TransferEvent::new(
            id,
            "convert",
            folder,
            paths.len() as u64,
            paths.len() as u64 * 1000,
        );
        event.label = job_label(&options, paths.len());
        crate::remote::transfer::emit(&app, &event);

        let hardware = if options.wants_hardware() {
            tools::detect_hardware(&tools)
        } else {
            None
        };
        let mut last_emit: Option<Instant> = None;
        let outcome = run::run_batch_with(
            &tools,
            &paths,
            &options,
            hardware,
            &suffix,
            replace_original,
            &|| token.is_cancelled(),
            &|path| trash::delete(path).map_err(|error| error.to_string()),
            &mut |index, _total, fraction| {
                if fraction < 1.0 && last_emit.is_some_and(|at| at.elapsed() < Duration::from_millis(100)) {
                    return;
                }
                last_emit = Some(Instant::now());
                event.files_done = index as u64;
                event.bytes_done = ((index as f64 + fraction) * 1000.0) as u64;
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
        if !outcome.cancelled {
            event.files_done = paths.len() as u64;
            event.bytes_done = event.bytes_total;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use options::{AudioFormat, Trim};

    #[test]
    fn suffixes_cannot_escape_the_folder() {
        assert!(validate_suffix("").is_ok());
        assert!(validate_suffix(" (compressed)").is_ok());
        assert!(validate_suffix("../evil").is_err());
        assert!(validate_suffix("a\\b").is_err());
        assert!(validate_suffix("tab\there").is_err());
    }

    #[test]
    fn jobs_are_named_for_what_they_do() {
        let audio = TranscodeOptions::Audio {
            format: AudioFormat::Mp3,
            bitrate: 128,
            trim: Trim::default(),
        };
        assert_eq!(job_label(&audio, 1), "Convert audio");
        assert_eq!(job_label(&audio, 3), "Convert 3 audios");
    }
}
