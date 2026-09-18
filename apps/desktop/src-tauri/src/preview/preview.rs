use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::explorer::entries::{coordinated_read, epoch_millis, utf8_boundary};
use crate::{explorer::archive, network, remote};

pub const PREVIEW_SNIFF_BYTES: usize = 8 * 1024;
pub const PREVIEW_MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PreviewKind {
    Text,
    Binary,
    Directory,
    Image,
    Archive,
    Pdf,
    Video,
    Audio,
}

#[derive(Serialize, Debug)]
pub struct FilePreview {
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) created: Option<u64>,
    pub(crate) modified: Option<u64>,
    pub(crate) kind: PreviewKind,
    pub(crate) content: Option<String>,
    pub(crate) src: Option<String>,
    pub(crate) truncated: bool,
}

pub fn image_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

pub fn video_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" => Some("video/mp4"),
        "mov" => Some("video/quicktime"),
        "webm" => Some("video/webm"),
        "ogv" => Some("video/ogg"),
        "mkv" => Some("video/x-matroska"),
        "avi" => Some("video/x-msvideo"),
        _ => None,
    }
}

pub fn audio_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => Some("audio/mpeg"),
        "m4a" => Some("audio/mp4"),
        "aac" => Some("audio/aac"),
        "wav" => Some("audio/wav"),
        "flac" => Some("audio/flac"),
        "ogg" | "oga" => Some("audio/ogg"),
        "opus" => Some("audio/opus"),
        _ => None,
    }
}

/// The content type the media protocol serves for a file, by extension.
pub(crate) fn media_mime(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    if let Some(mime) = image_mime(extension) {
        mime
    } else if let Some(mime) = video_mime(extension) {
        mime
    } else if let Some(mime) = audio_mime(extension) {
        mime
    } else if extension.eq_ignore_ascii_case("pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

/// Preview kind for extensions that stream through the media protocol instead of
/// being loaded into memory. Images, PDFs, video and audio all qualify.
pub(crate) fn media_preview_kind(extension: &str) -> Option<PreviewKind> {
    if extension.eq_ignore_ascii_case("pdf") {
        Some(PreviewKind::Pdf)
    } else if image_mime(extension).is_some() {
        Some(PreviewKind::Image)
    } else if video_mime(extension).is_some() {
        Some(PreviewKind::Video)
    } else if audio_mime(extension).is_some() {
        Some(PreviewKind::Audio)
    } else {
        None
    }
}

pub fn file_preview(path: &Path) -> Result<FilePreview, String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let mut preview = FilePreview {
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string()),
        size: metadata.len(),
        created: epoch_millis(metadata.created()),
        modified: epoch_millis(metadata.modified()),
        kind: PreviewKind::Directory,
        content: None,
        src: None,
        truncated: false,
    };
    if metadata.is_dir() {
        return Ok(preview);
    }

    if archive::is_archive_path(path) {
        preview.kind = PreviewKind::Archive;
        return Ok(preview);
    }

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        // Media is streamed on demand through the `media://` protocol; the preview
        // only reports the kind so the frontend can ask for a URL.
        if let Some(kind) = media_preview_kind(extension) {
            preview.kind = kind;
            return Ok(preview);
        }
    }

    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = file.take(PREVIEW_MAX_BYTES as u64 + 1);
    let mut bytes = Vec::new();
    (&mut reader)
        .take(PREVIEW_SNIFF_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    // A PDF without the extension: the magic bytes carry the whole file.
    if bytes.starts_with(b"%PDF-") {
        preview.kind = PreviewKind::Pdf;
        return Ok(preview);
    }
    if bytes.contains(&0) {
        preview.kind = PreviewKind::Binary;
        return Ok(preview);
    }

    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > PREVIEW_MAX_BYTES {
        bytes.truncate(utf8_boundary(&bytes, PREVIEW_MAX_BYTES));
        preview.truncated = true;
    }
    preview.kind = PreviewKind::Text;
    preview.content = Some(String::from_utf8_lossy(&bytes).into_owned());
    Ok(preview)
}

#[tauri::command]
pub async fn read_file_preview(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<FilePreview, String> {
    if network::servers::is_server_path(&path) {
        return network::servers::file_preview(&database.0, &sessions, &path).await;
    }
    if remote::is_remote_path(&path) {
        return remote::file_preview(&database.0, &clients, &path).await;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(path);
        coordinated_read(&path, || file_preview(&path))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn preview_fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "liteexplorer-preview-{}-{}",
            name,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn file_preview_marks_archives_without_reading_them() {
        let path = preview_fixture("bundle.zip", b"not really a zip");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Archive);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_reads_text_files() {
        let path = preview_fixture("notes.txt", b"hello\nworld");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.name, "notes.txt");
        assert_eq!(preview.size, 11);
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("hello\nworld"));
        assert!(!preview.truncated);
        assert!(preview.modified.is_some());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_detects_binary_files_by_nul_byte() {
        let path = preview_fixture("blob.bin", b"\x89PNG\r\n\x1a\n\x00\x00");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Binary);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_reads_empty_files_as_text() {
        let path = preview_fixture("empty.txt", b"");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some(""));
        assert_eq!(preview.size, 0);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_images_for_streaming() {
        let path = preview_fixture("photo.png", b"\x89PNG\r\n\x1a\n");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Image);
        assert_eq!(preview.content, None);
        assert_eq!(preview.src, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_video_and_audio_for_streaming() {
        let video = preview_fixture("clip.mp4", b"\x00\x00\x00 ftypmp42");
        let preview = file_preview(&video).unwrap();
        assert_eq!(preview.kind, PreviewKind::Video);
        assert_eq!(preview.src, None);
        fs::remove_dir_all(video.parent().unwrap()).unwrap();

        let audio = preview_fixture("song.flac", b"\x00fLaC");
        let preview = file_preview(&audio).unwrap();
        assert_eq!(preview.kind, PreviewKind::Audio);
        fs::remove_dir_all(audio.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_pdfs_for_streaming() {
        let path = preview_fixture("doc.pdf", b"%PDF-1.7\n\x00\x00binary body");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Pdf);
        assert_eq!(preview.content, None);
        assert_eq!(preview.src, None);
        assert!(!preview.truncated);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_detects_pdf_by_magic_without_extension() {
        let path = preview_fixture("report", b"%PDF-1.4\nno extension here");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Pdf);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn media_mime_maps_known_extensions() {
        assert_eq!(media_mime(Path::new("a.png")), "image/png");
        assert_eq!(media_mime(Path::new("a.mp4")), "video/mp4");
        assert_eq!(media_mime(Path::new("a.mp3")), "audio/mpeg");
        assert_eq!(media_mime(Path::new("a.pdf")), "application/pdf");
        assert_eq!(media_mime(Path::new("a.bin")), "application/octet-stream");
    }

    #[test]
    fn file_preview_decodes_non_utf8_text_lossily() {
        let path = preview_fixture("latin1.txt", b"caf\xe9");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("caf\u{FFFD}"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_truncates_large_files_on_a_utf8_boundary() {
        let mut bytes = vec![b'a'; PREVIEW_MAX_BYTES - 1];
        bytes.extend_from_slice("é".as_bytes());
        bytes.push(b'b');
        let path = preview_fixture("large.txt", &bytes);

        let preview = file_preview(&path).unwrap();

        assert!(preview.truncated);
        let content = preview.content.unwrap();
        assert_eq!(content.len(), PREVIEW_MAX_BYTES - 1);
        assert!(!content.contains('\u{FFFD}'));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_returns_directory_metadata_without_content() {
        let path = preview_fixture("inner.txt", b"x");
        let directory = path.parent().unwrap();

        let preview = file_preview(directory).unwrap();

        assert_eq!(preview.kind, PreviewKind::Directory);
        assert_eq!(preview.content, None);
        assert!(!preview.truncated);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn file_preview_errors_for_missing_files() {
        assert!(file_preview(Path::new("/definitely/missing/liteexplorer.txt")).is_err());
    }
}
