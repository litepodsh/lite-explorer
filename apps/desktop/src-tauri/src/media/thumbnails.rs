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
        let image = ImageReader::open(&source_path)
            .ok()?
            .with_guessed_format()
            .ok()?
            .decode()
            .ok()?;
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
    #[test]
    fn cache_key_uses_revision_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let metadata = file.as_file().metadata().unwrap();
        assert_eq!(cache_key(file.path(), &metadata).len(), 64);
    }
}
