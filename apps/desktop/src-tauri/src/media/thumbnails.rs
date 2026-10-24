use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use image::{ImageFormat, ImageReader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tokio::{sync::Semaphore, task::JoinSet};

use super::{register_local, MediaRegistry};
use crate::explorer::convert::{
    decode::decode_file,
    transform::{apply, Resize, Transform},
};

const EDGE: u32 = 256;
const MAX_SOURCE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageThumbnail {
    pub path: String,
    pub url: String,
}

fn modified_ms(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_millis())
}

pub fn cache_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mut hash = Sha256::new();
    hash.update(b"liteexplorer-thumbnail-v1\0");
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified_ms(metadata).to_le_bytes());
    hash.update(EDGE.to_le_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn trim_cache(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then(|| (entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    let mut total = files.iter().map(|(_, size, _)| size).sum::<u64>();
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in files {
        if total <= MAX_CACHE_BYTES {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// Formats the web view cannot display (HEIC, AVIF). They are decoded in Rust and shown
/// as a cached PNG, in the viewer and in thumbnails.
pub fn needs_decode(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        ["heic", "heif", "avif"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

/// Decodes `source` at full size and applies its EXIF orientation.
fn decode_upright(source: &Path) -> Result<image::DynamicImage, String> {
    let decoded = decode_file(source, Some(1.0))?;
    Ok(apply(
        decoded.image,
        &Transform {
            orientation: decoded.orientation,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            resize: Resize::None,
        },
    ))
}

fn preview_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mut hash = Sha256::new();
    hash.update(b"liteexplorer-preview-v1\0");
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified_ms(metadata).to_le_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A full-size PNG of a HEIC or AVIF file, cached by path, size and modified time.
pub fn decoded_png(cache_root: &Path, source: &Path) -> Result<PathBuf, String> {
    let metadata = fs::metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return Err("File too large to preview".into());
    }
    let dir = cache_root.join("previews");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let target = dir.join(format!("{}.png", preview_key(source, &metadata)));
    if !target.exists() {
        let image = decode_upright(source)?;
        let temporary = target.with_extension("tmp");
        image
            .save_with_format(&temporary, ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
        trim_cache(&dir);
    }
    Ok(target)
}

fn make_thumbnail(cache: &Path, source: &str) -> Option<(String, PathBuf)> {
    let source_path = fs::canonicalize(source).ok()?;
    let metadata = fs::metadata(&source_path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return None;
    }
    if source_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return Some((source.to_owned(), source_path));
    }
    let target = cache.join(format!("{}.png", cache_key(&source_path, &metadata)));
    if !target.exists() {
        let image = if needs_decode(&source_path) {
            decode_upright(&source_path).ok()?
        } else {
            ImageReader::open(&source_path)
                .ok()?
                .with_guessed_format()
                .ok()?
                .decode()
                .ok()?
        };
        let thumbnail = image.thumbnail(EDGE, EDGE);
        let temporary = target.with_extension("tmp");
        thumbnail
            .save_with_format(&temporary, ImageFormat::Png)
            .ok()?;
        fs::rename(temporary, &target).ok()?;
        trim_cache(cache);
    }
    Some((source.to_owned(), target))
}

#[tauri::command]
pub async fn image_thumbnails(
    app: AppHandle,
    registry: State<'_, MediaRegistry>,
    paths: Vec<String>,
) -> Result<Vec<ImageThumbnail>, String> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumbnails");
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
    let permits = std::sync::Arc::new(Semaphore::new(4));
    let mut jobs = JoinSet::new();
    for path in paths {
        let permit = permits
            .clone()
            .acquire_owned()
            .await
            .map_err(|error| error.to_string())?;
        let cache = cache.clone();
        jobs.spawn_blocking(move || {
            let _permit = permit;
            make_thumbnail(&cache, &path)
        });
    }
    let mut generated = Vec::new();
    while let Some(result) = jobs.join_next().await {
        if let Ok(Some(thumbnail)) = result {
            generated.push(thumbnail);
        }
    }
    Ok(generated
        .into_iter()
        .filter_map(|(path, file)| {
            register_local(&registry, file)
                .ok()
                .map(|media| ImageThumbnail {
                    path,
                    url: media.url,
                })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::convert::{encode::encode, formats::ImageFormat as Format};
    use image::{DynamicImage, Rgba, RgbaImage};

    fn avif_file(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([20, 120, 220, 255])));
        let path = dir.join(name);
        fs::write(&path, encode(&image, Format::Avif, 80).unwrap()).unwrap();
        path
    }

    #[test]
    fn only_heic_heif_and_avif_need_decoding() {
        for name in ["a.heic", "A.HEIC", "b.heif", "c.avif", "/x/y/d.AVIF"] {
            assert!(needs_decode(Path::new(name)), "{name}");
        }
        for name in ["a.png", "b.jpg", "c.svg", "d", "e.heic.txt"] {
            assert!(!needs_decode(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn decoded_png_is_full_size_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let source = avif_file(dir.path(), "photo.avif", 300, 200);
        let cache = dir.path().join("cache");
        let first = decoded_png(&cache, &source).unwrap();
        assert_eq!(image::image_dimensions(&first).unwrap(), (300, 200));
        let stamp = fs::metadata(&first).unwrap().modified().unwrap();
        let second = decoded_png(&cache, &source).unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::metadata(&second).unwrap().modified().unwrap(), stamp);
    }

    #[test]
    fn decoded_png_reports_unreadable_files() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.heic");
        fs::write(&broken, b"not a heic file").unwrap();
        assert!(decoded_png(&dir.path().join("cache"), &broken).is_err());
    }

    #[test]
    fn thumbnails_are_made_for_avif() {
        let dir = tempfile::tempdir().unwrap();
        let source = avif_file(dir.path(), "photo.avif", 600, 400);
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let (_, thumbnail) = make_thumbnail(&cache, source.to_str().unwrap()).unwrap();
        let (width, height) = image::image_dimensions(&thumbnail).unwrap();
        assert!(width <= EDGE && height <= EDGE && width.max(height) == EDGE, "{width}x{height}");
    }

    /// Manual check: `LITE_EXPLORER_HEIC_SAMPLE=/path/photo.heic cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_heic_preview_and_thumbnail() {
        let source = PathBuf::from(std::env::var("LITE_EXPLORER_HEIC_SAMPLE").expect("sample path"));
        let dir = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let png = decoded_png(dir.path(), &source).unwrap();
        println!(
            "preview: {:?}, {} bytes, {:?}",
            image::image_dimensions(&png).unwrap(),
            fs::metadata(&png).unwrap().len(),
            started.elapsed()
        );
        let cache = dir.path().join("thumbs");
        fs::create_dir_all(&cache).unwrap();
        let started = std::time::Instant::now();
        let (_, thumbnail) = make_thumbnail(&cache, source.to_str().unwrap()).unwrap();
        println!("thumbnail: {:?}, {:?}", image::image_dimensions(&thumbnail).unwrap(), started.elapsed());
    }

    #[test]
    fn cache_key_uses_revision_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let metadata = file.as_file().metadata().unwrap();
        assert_eq!(cache_key(file.path(), &metadata).len(), 64);
    }
}
