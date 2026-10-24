//! Batch rename: a list of steps becomes new names, shown in a preview and applied all-or-nothing.

pub mod execute;
pub mod plan;
pub mod steps;

use std::{
    fs,
    io::BufReader,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Local, NaiveDateTime};
use serde::Serialize;

use crate::app::error::AppError;
use crate::explorer::entries::coordinated_write;
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use plan::{PlanItem, PlanOutput, Status};
use steps::{needs_exif, Meta, RenameStep};

fn local_time(time: SystemTime) -> NaiveDateTime {
    DateTime::<Local>::from(time).naive_local()
}

/// `DateTimeOriginal` of a photo, read from the start of the file only.
fn exif_date(path: &Path) -> Option<NaiveDateTime> {
    let file = fs::File::open(path).ok()?;
    let data = exif::Reader::new()
        .read_from_container(&mut BufReader::new(file))
        .ok()?;
    let field = data.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
    NaiveDateTime::parse_from_str(&field.display_value().to_string(), "%Y-%m-%d %H:%M:%S").ok()
}

fn load_items(paths: &[String], steps: &[RenameStep]) -> Result<Vec<PlanItem>, String> {
    let read_exif = needs_exif(steps);
    paths
        .iter()
        .map(|path| {
            let path = validate_existing(Path::new(path), ExpectedKind::Any)?;
            let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
            let parent = path.parent().ok_or("Invalid local path")?;
            Ok(PlanItem {
                path: path.to_string_lossy().into_owned(),
                parent: parent.to_string_lossy().into_owned(),
                name: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .ok_or("Invalid local path")?,
                is_dir: metadata.is_dir(),
                meta: Meta {
                    created: metadata.created().ok().map(local_time),
                    modified: metadata.modified().ok().map(local_time),
                    exif: if read_exif && metadata.is_file() {
                        exif_date(&path)
                    } else {
                        None
                    },
                },
            })
        })
        .collect()
}

fn exists_in(parent: &str, name: &str) -> bool {
    Path::new(parent).join(name).symlink_metadata().is_ok()
}

fn compute_plan(paths: &[String], steps: &[RenameStep]) -> Result<PlanOutput, String> {
    let items = load_items(paths, steps)?;
    Ok(plan::plan(&items, steps, Local::now().naive_local(), &exists_in))
}

/// The preview: new names and a status for each item, in the order given.
#[tauri::command]
#[tracing::instrument(skip_all, name = "rename_plan", fields(sentry_op = "rename.plan"))]
pub async fn rename_plan(paths: Vec<String>, steps: Vec<RenameStep>) -> Result<PlanOutput, AppError> {
    tauri::async_runtime::spawn_blocking(move || compute_plan(&paths, &steps))
        .await
        .map_err(crate::app::error::from_join)?
        .map_err(AppError::from)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Renamed {
    pub from: String,
    pub to: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchOutcome {
    pub renamed: Vec<Renamed>,
}

fn run_batch(
    app: &tauri::AppHandle,
    job_id: String,
    paths: &[String],
    steps: &[RenameStep],
) -> Result<BatchOutcome, String> {
    // The plan is computed again here, so only validated names are ever applied.
    let output = compute_plan(paths, steps)?;
    if !output.can_apply() {
        return Err("The names changed since the preview. Review them and try again.".into());
    }
    let moves: Vec<execute::Move> = output
        .items
        .iter()
        .filter(|item| item.status == Status::Ok)
        .map(|item| {
            let from = PathBuf::from(&item.path);
            let to = from.with_file_name(&item.new_name);
            execute::Move { from, to }
        })
        .collect();

    let folder = moves
        .first()
        .and_then(|step| step.from.parent())
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut event = crate::remote::transfer::TransferEvent::new(job_id, "rename", folder, 0, 0);
    event.label = format!("Rename {} items", moves.len());
    event.files_total = moves.len() as u64;
    crate::remote::transfer::emit(app, &event);

    let result = execute::execute(
        &moves,
        &|from, to| {
            coordinated_write(from, || {
                fs::rename(from, to).map_err(|error| error.to_string())
            })
        },
        &mut |done, total| {
            event.files_done = done as u64;
            event.files_total = total as u64;
            crate::remote::transfer::emit(app, &event);
        },
    );

    match result {
        Ok(()) => {
            event.state = "done".into();
            crate::remote::transfer::emit(app, &event);
            Ok(BatchOutcome {
                renamed: moves
                    .iter()
                    .map(|step| Renamed {
                        from: step.from.to_string_lossy().into_owned(),
                        to: step.to.to_string_lossy().into_owned(),
                    })
                    .collect(),
            })
        }
        Err(failure) => {
            let message = if failure.restore_errors.is_empty() {
                format!(
                    "Couldn’t rename “{}”: {}. No files were changed.",
                    failure.name, failure.reason
                )
            } else {
                format!(
                    "Couldn’t rename “{}”: {}. Some files could not be put back: {}",
                    failure.name,
                    failure.reason,
                    failure.restore_errors.join("; ")
                )
            };
            event.state = "failed".into();
            event.error = Some(message.clone());
            crate::remote::transfer::emit(app, &event);
            Err(message)
        }
    }
}

/// Renames everything or nothing. Progress goes out as `transfer-progress` events.
#[tauri::command]
#[tracing::instrument(skip_all, name = "rename_batch", fields(sentry_op = "rename.batch"))]
pub async fn rename_batch(
    app: tauri::AppHandle,
    job_id: String,
    paths: Vec<String>,
    steps: Vec<RenameStep>,
) -> Result<BatchOutcome, AppError> {
    tauri::async_runtime::spawn_blocking(move || run_batch(&app, job_id, &paths, &steps))
        .await
        .map_err(crate::app::error::from_join)?
        .map_err(AppError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::rename::steps::{Position, StepKind};
    use std::fs;

    fn insert(text: &str) -> RenameStep {
        RenameStep {
            enabled: true,
            kind: StepKind::Insert {
                text: text.into(),
                position: Position::End,
            },
        }
    }

    fn paths(dir: &Path, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .map(|name| dir.join(name).to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn the_plan_reads_the_real_folder() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "A").unwrap();
        fs::write(dir.path().join("a_new.txt"), "existing").unwrap();
        let output = compute_plan(&paths(dir.path(), &["a.txt"]), &[insert("_new")]).unwrap();
        assert_eq!(output.items[0].new_name, "a_new.txt");
        // The new name is already taken by a file outside the batch.
        assert_eq!(output.items[0].status, Status::Conflict);
        assert!(!output.can_apply());
    }

    #[test]
    fn the_plan_marks_free_names_ok() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "A").unwrap();
        fs::write(dir.path().join("b.txt"), "B").unwrap();
        let output = compute_plan(&paths(dir.path(), &["a.txt", "b.txt"]), &[insert("_x")]).unwrap();
        assert!(output.can_apply());
        assert!(output.items.iter().all(|item| item.status == Status::Ok));
    }

    #[test]
    fn missing_files_are_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(compute_plan(&paths(dir.path(), &["ghost.txt"]), &[insert("x")]).is_err());
    }

    #[test]
    fn exif_dates_are_read_from_photos() {
        use crate::explorer::convert::{encode::encode, formats::ImageFormat};
        let dir = tempfile::tempdir().unwrap();
        // A little-endian TIFF block whose only IFD0 entry is DateTimeOriginal (0x9003 is in the
        // Exif IFD, so build the pointer: IFD0 -> Exif IFD -> DateTimeOriginal).
        let mut exif_block = Vec::new();
        exif_block.extend_from_slice(b"II*\0");
        exif_block.extend_from_slice(&8u32.to_le_bytes());
        exif_block.extend_from_slice(&1u16.to_le_bytes());
        exif_block.extend_from_slice(&0x8769u16.to_le_bytes());
        exif_block.extend_from_slice(&4u16.to_le_bytes());
        exif_block.extend_from_slice(&1u32.to_le_bytes());
        exif_block.extend_from_slice(&26u32.to_le_bytes());
        exif_block.extend_from_slice(&0u32.to_le_bytes());
        exif_block.extend_from_slice(&1u16.to_le_bytes());
        exif_block.extend_from_slice(&0x9003u16.to_le_bytes());
        exif_block.extend_from_slice(&2u16.to_le_bytes());
        exif_block.extend_from_slice(&20u32.to_le_bytes());
        exif_block.extend_from_slice(&44u32.to_le_bytes());
        exif_block.extend_from_slice(&0u32.to_le_bytes());
        exif_block.extend_from_slice(b"2024:08:12 14:03:11\0");
        let jpeg = encode(
            &image::DynamicImage::ImageRgba8(image::RgbaImage::new(4, 4)),
            ImageFormat::Jpeg,
            80,
        )
        .unwrap();
        let photo = dir.path().join("p.jpg");
        fs::write(&photo, crate::explorer::convert::metadata::with_exif(jpeg, &exif_block)).unwrap();
        assert_eq!(
            exif_date(&photo).map(|date| date.to_string()),
            Some("2024-08-12 14:03:11".to_string())
        );
        assert_eq!(exif_date(&dir.path().join("missing.jpg")), None);
    }
}
