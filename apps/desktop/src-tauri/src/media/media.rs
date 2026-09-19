//! Streams images, PDFs, video and audio to the webview over a custom `media://`
//! protocol. Files are read on demand with HTTP range support, so the webview never
//! holds a whole file in memory — the fix for huge files that used to stall previews.
//!
//! Local files are read from disk. S3 objects are proxied with ranged GETs. SFTP and
//! FTP files are downloaded to the app cache once, then served from there.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use serde::Serialize;
use tauri::{
    http::{header, HeaderValue, Method, Response, StatusCode},
    AppHandle, Manager, Runtime, State,
};

use crate::{remote, Database};

/// Largest body returned in a single response. The client asks for the rest with
/// follow-up range requests, which keeps memory flat for multi-gigabyte files.
const MEDIA_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

/// How long the protocol handler waits on a remote session before giving up.
const REMOTE_TIMEOUT: Duration = Duration::from_secs(60);

/// Where a media token reads from.
#[derive(Clone)]
enum MediaSource {
    Local(PathBuf),
    S3 {
        id: String,
        bucket: String,
        key: String,
        size: u64,
    },
    /// A file inside a zip package (EPUB, Office); `inner` comes from the request path.
    Zip(PathBuf),
}

/// Maps random tokens to their source. Tokens are handed to the webview and are the
/// only thing exposed in the URL, so file paths never reach the frontend or the DOM.
#[derive(Default)]
pub struct MediaRegistry(Mutex<HashMap<String, MediaSource>>);

impl MediaRegistry {
    fn register(&self, source: MediaSource) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        self.0.lock().unwrap().insert(token.clone(), source);
        token
    }

    fn get(&self, token: &str) -> Option<MediaSource> {
        self.0.lock().unwrap().get(token).cloned()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaUrl {
    pub url: String,
    pub size: u64,
}

fn media_url_for(token: &str) -> String {
    // Custom schemes have the same origin form as Tauri's asset protocol.
    #[cfg(windows)]
    {
        format!("http://media.localhost/{token}")
    }
    #[cfg(not(windows))]
    {
        format!("media://localhost/{token}")
    }
}

/// Resolves a path to a streamable URL: local paths are registered directly, S3
/// objects are proxied with ranges, and SFTP/FTP files are cached first.
#[tauri::command]
pub async fn media_url(
    app: AppHandle,
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, crate::network::servers::Sessions>,
    registry: State<'_, MediaRegistry>,
    path: String,
) -> Result<MediaUrl, String> {
    if crate::network::servers::is_server_path(&path) {
        let local =
            crate::network::servers::download_media_to_cache(&app, &database.0, &sessions, &path)
                .await?;
        return register_local(&registry, PathBuf::from(local));
    }
    if remote::is_remote_path(&path) {
        let (id, bucket, key, size) = remote::object_size(&database.0, &clients, &path).await?;
        let token = registry.register(MediaSource::S3 {
            id,
            bucket,
            key,
            size,
        });
        return Ok(MediaUrl {
            url: media_url_for(&token),
            size,
        });
    }
    register_local(&registry, PathBuf::from(path))
}

fn register_local(registry: &MediaRegistry, path: PathBuf) -> Result<MediaUrl, String> {
    let size = std::fs::metadata(&path)
        .map(|metadata| metadata.len())
        .map_err(|error| error.to_string())?;
    let token = registry.register(MediaSource::Local(path));
    Ok(MediaUrl {
        url: media_url_for(&token),
        size,
    })
}

/// Registers a zip package (EPUB, Office) as a `media://` root. Inner entries are
/// addressed by appending `/<inner path>` to the returned base URL.
pub fn register_zip_root(registry: &MediaRegistry, path: PathBuf) -> String {
    let token = registry.register(MediaSource::Zip(path));
    media_url_for(&token)
}

/// Registers the `media` scheme on the app builder. Wired up in `run`.
pub fn register<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol("media", |ctx, request, responder| {
        let app = ctx.app_handle().clone();
        if request.method() == Method::OPTIONS {
            responder.respond(preflight());
            return;
        }
        let path = request.uri().path().trim_start_matches('/').to_string();
        let (token, inner) = match path.split_once('/') {
            Some((token, inner)) => (token.to_string(), decode_percent(inner)),
            None => (path, String::new()),
        };
        let range = request
            .headers()
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        tauri::async_runtime::spawn(async move {
            responder.respond(handle(&app, &token, inner, range).await);
        });
    })
}

/// Percent-decodes a URL path segment run, leaving other bytes untouched.
fn decode_percent(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            if let Ok(byte) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn handle<R: Runtime>(
    app: &AppHandle<R>,
    token: &str,
    inner: String,
    range: Option<String>,
) -> Response<Vec<u8>> {
    let Some(source) = app.state::<MediaRegistry>().get(token) else {
        return error_response(StatusCode::NOT_FOUND, "Unknown media token");
    };
    match source {
        MediaSource::Local(path) => read_local(&path, range).await,
        MediaSource::S3 {
            id,
            bucket,
            key,
            size,
        } => read_s3(app, &id, &bucket, &key, size, range).await,
        MediaSource::Zip(path) => read_zip_member(&path, &inner, range).await,
    }
}

/// Serves one file from inside a zip package. Entries are small (images, CSS,
/// fonts), so the whole body is returned with a plain `GET`.
async fn read_zip_member(path: &Path, inner: &str, _range: Option<String>) -> Response<Vec<u8>> {
    let owned = path.to_path_buf();
    let name = inner.to_string();
    let bytes = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let file = File::open(&owned).map_err(|error| error.to_string())?;
        let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file))
            .map_err(|error| error.to_string())?;
        let mut entry = archive.by_name(&name).map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        Ok(bytes)
    })
    .await;
    match bytes {
        Ok(Ok(bytes)) => {
            let size = bytes.len() as u64;
            let resolved = ResolvedRange {
                start: 0,
                end: size.saturating_sub(1),
                partial: false,
            };
            build_response(zip_mime(inner), size, &resolved, bytes)
        }
        Ok(Err(error)) => error_response(StatusCode::NOT_FOUND, &error),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

/// MIME type for a zip inner entry, by extension. Falls back to the shared media
/// mapping, with text styles added for package assets.
fn zip_mime(inner: &str) -> &'static str {
    match inner
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
    {
        Some(extension) if extension == "css" => "text/css",
        Some(extension) if extension == "xhtml" || extension == "html" || extension == "htm" => {
            "application/xhtml+xml"
        }
        Some(extension) if extension == "svg" => "image/svg+xml",
        Some(extension) if extension == "js" => "text/javascript",
        _ => crate::media_mime(std::path::Path::new(inner)),
    }
}

#[derive(Debug, PartialEq)]
struct ResolvedRange {
    start: u64,
    end: u64,
    partial: bool,
}

/// Parses an HTTP `Range` header into inclusive byte bounds.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    if size == 0 {
        return None;
    }
    let spec = header.trim().strip_prefix("bytes=")?;
    let first = spec.split(',').next()?.trim();
    let (start, end) = first.split_once('-')?;
    if start.is_empty() {
        let suffix: u64 = end.parse().ok()?;
        if suffix == 0 {
            return None;
        }
        let start = size.saturating_sub(suffix);
        Some((start, size - 1))
    } else {
        let start: u64 = start.parse().ok()?;
        if start >= size {
            return None;
        }
        let end = if end.is_empty() {
            size - 1
        } else {
            end.parse::<u64>().ok()?.min(size - 1)
        };
        (end >= start).then_some((start, end))
    }
}

/// Chooses the byte window to serve. A `Range` request is capped at
/// `MEDIA_CHUNK_BYTES` so memory stays flat; a plain GET (images, small files)
/// gets the whole body, as HTTP expects.
fn resolve_range(header: Option<&str>, size: u64) -> Option<ResolvedRange> {
    if size == 0 {
        return Some(ResolvedRange {
            start: 0,
            end: 0,
            partial: false,
        });
    }
    match header.and_then(|value| parse_range(value, size)) {
        Some((start, end)) => Some(ResolvedRange {
            start,
            end: end.min(start + MEDIA_CHUNK_BYTES - 1),
            partial: true,
        }),
        None => Some(ResolvedRange {
            start: 0,
            end: size - 1,
            partial: false,
        }),
    }
}

async fn read_local(path: &PathBuf, range: Option<String>) -> Response<Vec<u8>> {
    let size = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.len(),
        Err(_) => return error_response(StatusCode::NOT_FOUND, "File not found"),
    };
    let Some(resolved) = resolve_range(range.as_deref(), size) else {
        return range_not_satisfiable(size);
    };
    if size == 0 {
        return build_response(crate::media_mime(path), size, &resolved, Vec::new());
    }
    let start = resolved.start;
    let length = (resolved.end - resolved.start + 1) as usize;
    let owned = path.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        let mut file = File::open(&owned)?;
        file.seek(SeekFrom::Start(start))?;
        let mut buffer = vec![0u8; length];
        file.read_exact(&mut buffer)?;
        Ok(buffer)
    })
    .await;
    match bytes {
        Ok(Ok(bytes)) => build_response(crate::media_mime(path), size, &resolved, bytes),
        Ok(Err(error)) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn read_s3<R: Runtime>(
    app: &AppHandle<R>,
    id: &str,
    bucket: &str,
    key: &str,
    size: u64,
    range: Option<String>,
) -> Response<Vec<u8>> {
    let Some(resolved) = resolve_range(range.as_deref(), size) else {
        return range_not_satisfiable(size);
    };
    if size == 0 {
        return build_response("application/octet-stream", size, &resolved, Vec::new());
    }
    let database = app.state::<Database>();
    let clients = app.state::<remote::RemoteClients>();
    let request = format!("bytes={}-{}", resolved.start, resolved.end);
    let read = tokio::time::timeout(
        REMOTE_TIMEOUT,
        remote::read_object_range(&database.0, &clients, id, bucket, key, Some(request)),
    )
    .await;
    match read {
        Ok(Ok(bytes)) => {
            let mime = crate::media_mime(std::path::Path::new(key));
            build_response(mime, size, &resolved, bytes)
        }
        Ok(Err(error)) => error_response(StatusCode::BAD_GATEWAY, &error),
        Err(_) => error_response(StatusCode::GATEWAY_TIMEOUT, "Remote read timed out"),
    }
}

fn cors(response: tauri::http::response::Builder) -> tauri::http::response::Builder {
    response
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, OPTIONS"),
        )
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Range"),
        )
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("Content-Range, Accept-Ranges, Content-Length"),
        )
}

fn preflight() -> Response<Vec<u8>> {
    cors(Response::builder().status(StatusCode::NO_CONTENT))
        .body(Vec::new())
        .unwrap()
}

fn build_response(
    mime: &str,
    size: u64,
    range: &ResolvedRange,
    bytes: Vec<u8>,
) -> Response<Vec<u8>> {
    let length = bytes.len() as u64;
    let status = if range.partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let mut builder = cors(Response::builder().status(status))
        .header(header::CONTENT_TYPE, HeaderValue::from_str(mime).unwrap())
        .header(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"))
        .header(header::CONTENT_LENGTH, length.to_string());
    if range.partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{size}", range.start, range.end),
        );
    }
    builder.body(bytes).unwrap()
}

fn range_not_satisfiable(size: u64) -> Response<Vec<u8>> {
    cors(
        Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{size}")),
    )
    .body(Vec::new())
    .unwrap()
}

fn error_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    cors(Response::builder().status(status))
        .body(message.as_bytes().to_vec())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_range_handles_open_ended_and_closed_ranges() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
        assert_eq!(parse_range("bytes=200-", 1000), Some((200, 999)));
        assert_eq!(parse_range("bytes=-100", 1000), Some((900, 999)));
        assert_eq!(parse_range("bytes=0-2000", 1000), Some((0, 999)));
    }

    #[test]
    fn parse_range_rejects_malformed_and_out_of_bounds() {
        assert_eq!(parse_range("items=0-1", 1000), None);
        assert_eq!(parse_range("bytes=1000-", 1000), None);
        assert_eq!(parse_range("bytes=abc", 1000), None);
        assert_eq!(parse_range("bytes=-0", 1000), None);
        assert_eq!(parse_range("bytes=0-10", 0), None);
    }

    #[test]
    fn resolve_range_caps_chunks_and_marks_partial() {
        // No Range header: serve the whole file (needed by <img> for large images).
        let whole = resolve_range(None, MEDIA_CHUNK_BYTES * 3).unwrap();
        assert_eq!(whole.start, 0);
        assert_eq!(whole.end, MEDIA_CHUNK_BYTES * 3 - 1);
        assert!(!whole.partial);

        // A Range header is capped so a large video never buffers all at once.
        let capped = resolve_range(Some("bytes=0-"), MEDIA_CHUNK_BYTES * 3).unwrap();
        assert_eq!(capped.start, 0);
        assert_eq!(capped.end, MEDIA_CHUNK_BYTES - 1);
        assert!(capped.partial);

        let asked = resolve_range(Some("bytes=10-20"), 1024).unwrap();
        assert_eq!((asked.start, asked.end), (10, 20));
        assert!(asked.partial);
    }
}
