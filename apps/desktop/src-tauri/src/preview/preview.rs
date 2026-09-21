use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::explorer::archive::Codec;
use crate::explorer::entries::{coordinated_read, epoch_millis, utf8_boundary};
use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::search::text;
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
    Font,
    Epub,
    Spreadsheet,
    Rtf,
    Word,
    Presentation,
    Mail,
    Mbox,
    Contact,
    Calendar,
    Torrent,
    Data,
    Diff,
    Log,
    Comic,
    Notebook,
    Database,
    Subtitle,
    Certificate,
    Model,
    Geo,
    Fb2,
    Pcap,
    Iso,
    Msg,
    Sketch,
    Psd,
    Dicom,
    Mobi,
    Avro,
    Parquet,
    Arrow,
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

/// Font MIME types the webview will accept as a `@font-face` source. Wrong MIME
/// makes the browser silently drop the font, so these must be exact.
pub fn font_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "otf" => Some("font/otf"),
        "ttf" => Some("font/ttf"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        _ => None,
    }
}

/// MIME types for 3D models streamed to the `three.js` viewer.
pub fn model_mime(extension: &str) -> Option<&'static str> {
    match extension.to_ascii_lowercase().as_str() {
        "stl" => Some("model/stl"),
        "obj" => Some("model/obj"),
        "gltf" => Some("model/gltf+json"),
        "glb" => Some("model/gltf-binary"),
        "dae" => Some("model/vnd.collada+xml"),
        _ => None,
    }
}

/// Whether `extension` names an EPUB package, which previews through its own reader.
pub(crate) fn is_epub_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("epub")
}

/// Whether `extension` names a rich text document.
pub(crate) fn is_rtf_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("rtf")
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
    } else if let Some(mime) = font_mime(extension) {
        mime
    } else if let Some(mime) = model_mime(extension) {
        mime
    } else if extension.eq_ignore_ascii_case("pdf") || extension.eq_ignore_ascii_case("ai") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

/// Preview kind for extensions that stream through the media protocol instead of
/// being loaded into memory. Images, PDFs, video, audio, fonts and 3D models all qualify.
pub(crate) fn media_preview_kind(extension: &str) -> Option<PreviewKind> {
    if extension.eq_ignore_ascii_case("pdf") {
        Some(PreviewKind::Pdf)
    } else if image_mime(extension).is_some() {
        Some(PreviewKind::Image)
    } else if video_mime(extension).is_some() {
        Some(PreviewKind::Video)
    } else if audio_mime(extension).is_some() {
        Some(PreviewKind::Audio)
    } else if font_mime(extension).is_some() {
        Some(PreviewKind::Font)
    } else if model_mime(extension).is_some() {
        Some(PreviewKind::Model)
    } else {
        None
    }
}

/// Kind for a file implied by its extension alone, before any bytes are read.
/// Covers streamed media, fonts, the EPUB reader and spreadsheets.
pub(crate) fn preview_kind_for_extension(extension: &str) -> Option<PreviewKind> {
    if is_epub_extension(extension) {
        Some(PreviewKind::Epub)
    } else if is_rtf_extension(extension) {
        Some(PreviewKind::Rtf)
    } else if crate::preview::office::is_word_extension(extension) {
        Some(PreviewKind::Word)
    } else if crate::preview::office::is_presentation_extension(extension) {
        Some(PreviewKind::Presentation)
    } else if crate::preview::mail::is_mail_extension(extension) {
        Some(PreviewKind::Mail)
    } else if crate::preview::mail::is_mbox_extension(extension) {
        Some(PreviewKind::Mbox)
    } else if crate::preview::vcard::is_vcard_extension(extension) {
        Some(PreviewKind::Contact)
    } else if crate::preview::calendar::is_calendar_extension(extension) {
        Some(PreviewKind::Calendar)
    } else if crate::preview::torrent::is_torrent_extension(extension) {
        Some(PreviewKind::Torrent)
    } else if crate::preview::data::is_data_extension(extension) {
        Some(PreviewKind::Data)
    } else if crate::preview::comic::is_comic_extension(extension) {
        Some(PreviewKind::Comic)
    } else if crate::preview::notebook::is_notebook_extension(extension) {
        Some(PreviewKind::Notebook)
    } else if crate::preview::database::is_database_extension(extension) {
        Some(PreviewKind::Database)
    } else if crate::preview::subtitle::is_subtitle_extension(extension) {
        Some(PreviewKind::Subtitle)
    } else if crate::preview::certificate::is_certificate_extension(extension) {
        Some(PreviewKind::Certificate)
    } else if crate::preview::geo::is_geo_extension(extension) {
        Some(PreviewKind::Geo)
    } else if crate::preview::fb2::is_fb2_extension(extension) {
        Some(PreviewKind::Fb2)
    } else if crate::preview::pcap::is_pcap_extension(extension) {
        Some(PreviewKind::Pcap)
    } else if crate::preview::iso::is_iso_extension(extension) {
        Some(PreviewKind::Iso)
    } else if crate::preview::msg::is_msg_extension(extension) {
        Some(PreviewKind::Msg)
    } else if crate::preview::sketch::is_sketch_extension(extension) {
        Some(PreviewKind::Sketch)
    } else if crate::preview::psd::is_psd_extension(extension) {
        Some(PreviewKind::Psd)
    } else if crate::preview::dicom::is_dicom_extension(extension) {
        Some(PreviewKind::Dicom)
    } else if crate::preview::mobi::is_mobi_extension(extension) {
        Some(PreviewKind::Mobi)
    } else if crate::preview::avro::is_avro_extension(extension) {
        Some(PreviewKind::Avro)
    } else if crate::preview::parquet::is_parquet_extension(extension) {
        Some(PreviewKind::Parquet)
    } else if crate::preview::arrow::is_arrow_extension(extension) {
        Some(PreviewKind::Arrow)
    } else if crate::preview::sheet::is_spreadsheet_extension(extension) {
        Some(PreviewKind::Spreadsheet)
    } else {
        media_preview_kind(extension)
    }
}

/// Truncation point that keeps a UTF-16 stream on a code-unit boundary. UTF-8
/// text uses [`utf8_boundary`].
fn preview_boundary(bytes: &[u8], max: usize) -> usize {
    if matches!(bytes.first(), Some(0xFF | 0xFE))
        && matches!(bytes.get(..2), Some([0xFF, 0xFE]) | Some([0xFE, 0xFF]))
    {
        return max & !1;
    }
    utf8_boundary(bytes, max)
}

/// Shared text/binary classification for every preview path (local, remote,
/// server). Reuses the content-search decoder, so UTF-16 (with or without BOM)
/// and Windows-1252 files preview as text instead of being misread as binary.
pub(crate) fn classify_preview_bytes(preview: &mut FilePreview, mut bytes: Vec<u8>) {
    if bytes.len() > PREVIEW_MAX_BYTES {
        bytes.truncate(preview_boundary(&bytes, PREVIEW_MAX_BYTES));
        preview.truncated = true;
    }
    match text::decode(&bytes) {
        Some(content) => {
            preview.kind = PreviewKind::Text;
            preview.content = Some(content.into_owned());
        }
        None => preview.kind = PreviewKind::Binary,
    }
}

/// Refines a text preview into a richer reader based on the file extension
/// (colored diff, ANSI log). Files without a dedicated kind stay `Text`.
pub(crate) fn apply_text_extension_kind(preview: &mut FilePreview, name: &str) {
    if preview.kind != PreviewKind::Text {
        return;
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "diff" | "patch" => preview.kind = PreviewKind::Diff,
        "log" => preview.kind = PreviewKind::Log,
        _ => {}
    }
}

/// Decompresses a single-file archive (`app.log.gz`) into the preview, bounded to
/// [`PREVIEW_MAX_BYTES`].
fn decompress_preview(path: &Path, codec: Codec, preview: &mut FilePreview) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let reader =
        archive::decoder(codec, file).map_err(|_| "Unsupported compression".to_string())?;
    let mut bytes = Vec::new();
    reader
        .take(PREVIEW_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    classify_preview_bytes(preview, bytes);
    Ok(())
}

pub fn file_preview(path: &Path) -> Result<FilePreview, String> {
    let path = validate_existing(path, ExpectedKind::Any)?;
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
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

    if archive::is_archive_path(&path) {
        preview.kind = PreviewKind::Archive;
        return Ok(preview);
    }

    // Single compressed files (`app.log.gz`) preview their decompressed payload.
    if let Some((codec, _)) = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(archive::compressed)
    {
        decompress_preview(&path, codec, &mut preview)?;
        return Ok(preview);
    }

    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        // Media is streamed on demand through the `media://` protocol; the preview
        // only reports the kind so the frontend can ask for a URL. EPUB gets its
        // own reader that parses the package on demand.
        if let Some(kind) = preview_kind_for_extension(extension) {
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
    // Classify the sniffed head first so large binaries never get fully read.
    if text::decode(&bytes).is_none() {
        preview.kind = PreviewKind::Binary;
        return Ok(preview);
    }

    reader
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    classify_preview_bytes(&mut preview, bytes);
    let name = preview.name.clone();
    apply_text_extension_kind(&mut preview, &name);
    Ok(preview)
}

#[tauri::command]
#[tracing::instrument(skip_all, name = "read_file_preview", fields(sentry_op = "file.preview"))]
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
        let directory = std::env::temp_dir().canonicalize().unwrap().join(format!(
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
        assert_eq!(media_mime(Path::new("a.stl")), "model/stl");
        assert_eq!(media_mime(Path::new("a.glb")), "model/gltf-binary");
        assert_eq!(media_mime(Path::new("a.bin")), "application/octet-stream");
    }

    #[test]
    fn file_preview_decodes_non_utf8_text_lossily() {
        // Windows-1252 fallback: the legacy byte decodes as its real character.
        let path = preview_fixture("latin1.txt", b"caf\xe9");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("café"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    fn utf16le(text: &str, bom: bool) -> Vec<u8> {
        let mut bytes = if bom { vec![0xFF, 0xFE] } else { Vec::new() };
        bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        bytes
    }

    #[test]
    fn file_preview_decodes_utf16_text_instead_of_calling_it_binary() {
        let le = preview_fixture("lectura.txt", &utf16le("Medidor MED-77120", true));
        let preview = file_preview(&le).unwrap();
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("Medidor MED-77120"));
        fs::remove_dir_all(le.parent().unwrap()).unwrap();

        let be_bytes: Vec<u8> = "Ñusta Ayala"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        let be = preview_fixture("clientes.tsv", &be_bytes);
        let preview = file_preview(&be).unwrap();
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("Ñusta Ayala"));
        fs::remove_dir_all(be.parent().unwrap()).unwrap();

        // No BOM: detected from the NUL pattern of mostly-ASCII text.
        let bare = preview_fixture("registro.txt", &utf16le("acceso denegado", false));
        let preview = file_preview(&bare).unwrap();
        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("acceso denegado"));
        fs::remove_dir_all(bare.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_fonts_for_streaming() {
        let path = preview_fixture("gotham.otf", b"OTTO\x00\x01\x02\x03");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Font);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_epub_for_the_reader() {
        let path = preview_fixture("book.epub", b"PK\x03\x04zipped epub");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Epub);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_decompresses_single_compressed_files() {
        use flate2::{write::GzEncoder, Compression};
        use std::io::Write;

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"id,name\n1,Zarpa").unwrap();
        let path = preview_fixture("export.csv.gz", &encoder.finish().unwrap());

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Text);
        assert_eq!(preview.content.as_deref(), Some("id,name\n1,Zarpa"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_spreadsheet_without_reading_bytes() {
        let path = preview_fixture("report.xlsx", b"PK\x03\x04zipped sheet");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Spreadsheet);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_rtf_for_its_reader() {
        let path = preview_fixture("nota.rtf", br"{\rtf1\ansi Hola}");

        let preview = file_preview(&path).unwrap();

        assert_eq!(preview.kind, PreviewKind::Rtf);
        assert_eq!(preview.content, None);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_preview_marks_mail_and_mbox() {
        for (name, expected) in [
            ("message.eml", PreviewKind::Mail),
            ("apple.emlx", PreviewKind::Mail),
            ("inbox.mbox", PreviewKind::Mbox),
        ] {
            let path = preview_fixture(name, b"From: a@b\r\nSubject: hola\r\n\r\nbody");
            let preview = file_preview(&path).unwrap();
            assert_eq!(preview.kind, expected, "{name}");
            assert_eq!(preview.content, None);
            fs::remove_dir_all(path.parent().unwrap()).unwrap();
        }
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
