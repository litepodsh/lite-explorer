//! Portable EXIF inspection and lossless container editing; no platform utilities.
use super::local_path::{validate_existing, ExpectedKind};
use img_parts::{DynImage, ImageEXIF};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Serialize)]
pub struct ExifField {
    tag: String,
    description: String,
    group: String,
    value: String,
}
#[derive(Serialize)]
pub struct ExifInfo {
    fields: Vec<ExifField>,
    warning: Option<String>,
    can_remove: bool,
    removal_reason: Option<String>,
    revision: String,
}
fn read_image(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_IMAGE_BYTES {
        return Err("EXIF inspection supports images up to 256 MiB.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("Image exceeds the 256 MiB limit.".into());
    }
    Ok(bytes)
}
fn revision(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn inspect(path: &Path) -> Result<ExifInfo, String> {
    let path = validate_existing(path, ExpectedKind::File)?;
    let bytes = read_image(&path)?;
    let revision = revision(&bytes);
    let readonly = fs::metadata(&path)
        .map_err(|e| e.to_string())?
        .permissions()
        .readonly();
    let mut warning = None;
    let parsed = exif::Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut Cursor::new(&bytes))
        .or_else(|e| {
            e.distill_partial_result(|errors| {
                warning = Some(format!(
                    "Some EXIF fields could not be read ({} parsing errors).",
                    errors.len()
                ));
            })
        });
    let fields = match parsed {
        Ok(data) => data
            .fields()
            .map(|field| ExifField {
                tag: field.tag.to_string(),
                description: field
                    .tag
                    .description()
                    .unwrap_or("Unrecognized EXIF tag")
                    .into(),
                group: format!("{:?} · IFD {}", field.tag.context(), field.ifd_num.index()),
                value: field.display_value().with_unit(&data).to_string(),
            })
            .collect(),
        Err(exif::Error::NotFound(_)) => Vec::new(),
        Err(e) => {
            warning = Some(format!("EXIF could not be fully read: {e}"));
            Vec::new()
        }
    };
    let image = DynImage::from_bytes(bytes.into()).ok().flatten();
    let supported = image.is_some();
    let has_exif = image.as_ref().is_some_and(|image| image.exif().is_some());
    let removal_reason = if readonly {
        Some("This file is read-only.".into())
    } else if !supported {
        Some("EXIF removal supports JPEG, PNG and WebP files.".into())
    } else if !has_exif {
        Some("No EXIF block to remove.".into())
    } else {
        None
    };
    Ok(ExifInfo {
        fields,
        warning,
        can_remove: removal_reason.is_none(),
        removal_reason,
        revision,
    })
}

fn remove(path: &Path, expected_revision: &str) -> Result<(), String> {
    let path = validate_existing(path, ExpectedKind::File)?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.permissions().readonly() {
        return Err("This file is read-only.".into());
    }
    let bytes = read_image(&path)?;
    if revision(&bytes) != expected_revision {
        return Err("The file changed. Reopen Get Info before removing EXIF.".into());
    }
    let mut image = DynImage::from_bytes(bytes.into())
        .map_err(|e| e.to_string())?
        .ok_or("EXIF removal supports JPEG, PNG and WebP files.")?;
    if image.exif().is_none() {
        return Ok(());
    }
    image.set_exif(None);
    // Write beside the original, flush, and atomically replace only after successful encoding.
    let parent = path.parent().ok_or("The file has no parent directory.")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    image
        .encoder()
        .write_to(temporary.as_file_mut())
        .map_err(|e| e.to_string())?;
    temporary.as_file_mut().flush().map_err(|e| e.to_string())?;
    temporary
        .as_file()
        .set_permissions(meta.permissions())
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    // Do not overwrite edits made while encoding, or a path changed to a symlink.
    validate_existing(&path, ExpectedKind::File)?;
    if revision(&read_image(&path)?) != expected_revision {
        return Err("The file changed. Nothing was overwritten.".into());
    }
    temporary.persist(&path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn read_file_exif(path: String) -> Result<ExifInfo, String> {
    tauri::async_runtime::spawn_blocking(move || inspect(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn remove_file_exif(path: String, revision: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || remove(Path::new(&path), &revision))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    const JPEG: &[u8] = include_bytes!("fixtures/exif.jpeg");
    const PNG: &[u8] = include_bytes!("fixtures/exif.png");
    const WEBP: &[u8] = include_bytes!("fixtures/exif.webp");

    // Encoded pixel payload and unrelated metadata must survive byte for byte.
    fn payload(bytes: &[u8]) -> Vec<Vec<u8>> {
        match DynImage::from_bytes(bytes.to_vec().into())
            .unwrap()
            .unwrap()
        {
            DynImage::Jpeg(image) => image
                .segments()
                .iter()
                .filter(|s| !(s.marker() == 0xe1 && s.contents().starts_with(b"Exif\0\0")))
                .map(|s| s.clone().encoder().bytes().to_vec())
                .collect(),
            DynImage::Png(image) => image
                .chunks()
                .iter()
                .filter(|c| c.kind() != *b"eXIf")
                .map(|c| c.clone().encoder().bytes().to_vec())
                .collect(),
            DynImage::WebP(image) => image
                .chunks()
                .iter()
                .filter(|c| c.id() != *b"EXIF" && c.id() != *b"VP8X")
                .map(|c| c.clone().encoder().bytes().to_vec())
                .collect(),
        }
    }
    #[test]
    fn reads_all_fields_and_removes_exif_without_recompressing() {
        for (name, original) in [
            ("image.jpeg", JPEG),
            ("image.png", PNG),
            ("image.webp", WEBP),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().canonicalize().unwrap().join(name);
            fs::write(&path, original).unwrap();
            let before = inspect(&path).unwrap();
            assert!(before.fields.iter().any(
                |field| field.tag == "Make" && field.value.contains("LiteExplorer test camera")
            ));
            assert!(before.fields.iter().any(|field| field.tag == "DateTime"));
            assert!(before.can_remove, "{name}: {:?}", before.removal_reason);
            let content = payload(original);
            remove(&path, &before.revision).unwrap();
            let after = fs::read(&path).unwrap();
            assert_eq!(
                payload(&after),
                content,
                "pixel data or unrelated chunks changed: {name}"
            );
            assert!(DynImage::from_bytes(after.into())
                .unwrap()
                .unwrap()
                .exif()
                .is_none());
            let info = inspect(&path).unwrap();
            assert!(info.fields.is_empty());
            assert!(!info.can_remove);
            assert_eq!(
                fs::read_dir(dir.path()).unwrap().count(),
                1,
                "temporary files must be cleaned up"
            );
        }
    }
    #[test]
    fn stale_revision_and_corruption_never_overwrite_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("image.jpg");
        fs::write(&path, JPEG).unwrap();
        assert!(remove(&path, "stale").is_err());
        assert_eq!(fs::read(&path).unwrap(), JPEG);
        let corrupt = b"\xff\xd8\xff\xe1\x00\xffbroken";
        fs::write(&path, corrupt).unwrap();
        assert!(remove(&path, &revision(corrupt)).is_err());
        assert_eq!(fs::read(&path).unwrap(), corrupt);
    }
    #[test]
    fn rejects_readonly_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().canonicalize().unwrap().join("image.png");
        fs::write(&path, PNG).unwrap();
        let original_permissions = fs::metadata(&path).unwrap().permissions();
        let mut readonly = original_permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&path, readonly).unwrap();
        assert!(!inspect(&path).unwrap().can_remove);
        assert!(remove(&path, &revision(PNG)).is_err());
        assert_eq!(fs::read(&path).unwrap(), PNG);
        fs::set_permissions(&path, original_permissions).unwrap();
    }
}
