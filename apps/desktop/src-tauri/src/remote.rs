//! S3-compatible remote locations (AWS S3, Cloudflare R2, custom endpoints).
//!
//! Connection metadata lives in the `remote_locations` table; the secret access key
//! lives in the OS credential store (Keychain on macOS) and never reaches the frontend.
//! Remote paths use the scheme `s3://<location-id>/[<bucket>/<key prefix>]`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    sync::Mutex,
    time::Duration,
};

use aws_sdk_s3::{
    config::{
        http::HttpResponse, retry::RetryConfig, timeout::TimeoutConfig, BehaviorVersion,
        Credentials, Region, RequestChecksumCalculation, ResponseChecksumValidation,
    },
    error::{ProvideErrorMetadata, SdkError},
    primitives::DateTime,
    Client,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::{AppHandle, Manager, State};
use tokio::sync::OnceCell;

pub mod bucket_settings;
pub mod buckets;
pub mod write;

use super::{
    image_mime, now_secs, set_pdf_preview, utf8_boundary, Database, DirectoryEntry, FilePreview,
    Location, PreviewKind, IMAGE_MAX_BYTES, PDF_MAX_BYTES, PREVIEW_MAX_BYTES, PREVIEW_SNIFF_BYTES,
};

const KEYCHAIN_SERVICE: &str = "lite-explorer.s3";
const SCHEME: &str = "s3://";
/// Listing stops after this many entries so huge prefixes stay responsive.
const MAX_LISTING_ENTRIES: usize = 10_000;
/// Largest object "Open" downloads to the local cache.
const DOWNLOAD_MAX_BYTES: u64 = 200 * 1024 * 1024;

/// S3 clients per saved location, built on first use so the keychain is read once per session.
/// Each location gets one cell, so concurrent first requests share a single keychain read
/// (and a single macOS access prompt).
#[derive(Default)]
pub struct RemoteClients(Mutex<HashMap<String, Arc<OnceCell<Client>>>>);

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Aws,
    R2,
    Custom,
}

impl Provider {
    fn as_str(self) -> &'static str {
        match self {
            Provider::Aws => "aws",
            Provider::R2 => "r2",
            Provider::Custom => "custom",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Provider::Aws => "Amazon S3",
            Provider::R2 => "Cloudflare R2",
            Provider::Custom => "S3 Storage",
        }
    }
}

/// Form input sent by the add-location dialog.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoteLocationInput {
    provider: Provider,
    #[serde(default)]
    name: String,
    #[serde(default)]
    account_id: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    region: String,
    access_key_id: String,
    secret_access_key: String,
    #[serde(default)]
    bucket: String,
    #[serde(default)]
    prefix: String,
    #[serde(default)]
    path_style: bool,
}

/// Validated, normalized connection settings.
#[derive(Debug, Clone, PartialEq)]
struct Connection {
    provider: Provider,
    name: String,
    endpoint: Option<String>,
    region: String,
    bucket: Option<String>,
    prefix: Option<String>,
    access_key_id: String,
    path_style: bool,
}

#[derive(Serialize, Debug)]
pub struct ConnectionTest {
    /// Bucket names when the location spans the whole account, `None` for a single bucket.
    buckets: Option<Vec<String>>,
}

#[derive(Debug, PartialEq)]
pub struct RemotePath {
    pub id: String,
    pub bucket: Option<String>,
    pub key: String,
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_prefix(prefix: &str) -> Option<String> {
    let prefix = prefix.trim().trim_matches('/');
    (!prefix.is_empty()).then(|| format!("{prefix}/"))
}

fn r2_endpoint(account_id: &str) -> Result<String, String> {
    let account_id = account_id.trim().to_ascii_lowercase();
    if account_id.len() != 32 || !account_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Account ID must be the 32-character ID from the Cloudflare dashboard".into());
    }
    Ok(format!("https://{account_id}.r2.cloudflarestorage.com"))
}

fn custom_endpoint(endpoint: &str) -> Result<String, String> {
    let endpoint = endpoint.trim().trim_end_matches('/');
    if endpoint.is_empty() {
        return Err("Endpoint URL is required".into());
    }
    if !(endpoint.starts_with("https://") || endpoint.starts_with("http://")) {
        return Err("Endpoint URL must start with https:// or http://".into());
    }
    Ok(endpoint.to_string())
}

fn resolve(input: &RemoteLocationInput) -> Result<Connection, String> {
    let access_key_id = non_empty(&input.access_key_id).ok_or("Access key ID is required")?;
    if input.secret_access_key.is_empty() {
        return Err("Secret access key is required".into());
    }
    let bucket = non_empty(&input.bucket);
    if bucket.as_deref().is_some_and(|bucket| bucket.contains('/')) {
        return Err("Bucket name cannot contain /".into());
    }
    let prefix = bucket
        .as_ref()
        .and_then(|_| normalize_prefix(&input.prefix));

    let (endpoint, region, path_style) = match input.provider {
        Provider::Aws => (
            None,
            non_empty(&input.region).unwrap_or_else(|| "us-east-1".into()),
            false,
        ),
        Provider::R2 => (Some(r2_endpoint(&input.account_id)?), "auto".into(), true),
        Provider::Custom => (
            Some(custom_endpoint(&input.endpoint)?),
            non_empty(&input.region).unwrap_or_else(|| "us-east-1".into()),
            input.path_style,
        ),
    };

    let name = non_empty(&input.name)
        .or_else(|| bucket.clone())
        .unwrap_or_else(|| input.provider.label().to_string());

    Ok(Connection {
        provider: input.provider,
        name,
        endpoint,
        region,
        bucket,
        prefix,
        access_key_id,
        path_style,
    })
}

fn client(connection: &Connection, secret: &str) -> Client {
    let credentials = Credentials::new(
        &connection.access_key_id,
        secret,
        None,
        None,
        "lite-explorer",
    );
    let mut config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(connection.region.clone()))
        .credentials_provider(credentials)
        .force_path_style(connection.path_style)
        // Several S3-compatible services reject the newer default checksum headers.
        .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
        .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
        .retry_config(RetryConfig::standard().with_max_attempts(2))
        .timeout_config(
            TimeoutConfig::builder()
                .connect_timeout(Duration::from_secs(5))
                .operation_timeout(Duration::from_secs(20))
                .build(),
        );
    if let Some(endpoint) = &connection.endpoint {
        config = config.endpoint_url(endpoint);
    }
    Client::from_conf(config.build())
}

fn describe_error<E, R>(error: SdkError<E, R>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
    R: std::fmt::Debug,
{
    match &error {
        SdkError::DispatchFailure(failure) => {
            if failure.is_timeout() {
                "Connection timed out".into()
            } else {
                "Could not reach the endpoint. Check the URL and your network.".into()
            }
        }
        SdkError::TimeoutError(_) => "Connection timed out".into(),
        SdkError::ServiceError(context) => {
            let service = context.err();
            match (service.code(), service.message()) {
                (Some(code), Some(message)) => format!("{code}: {message}"),
                (Some(code), None) => code.to_string(),
                _ => aws_sdk_s3::error::DisplayErrorContext(&error).to_string(),
            }
        }
        _ => aws_sdk_s3::error::DisplayErrorContext(&error).to_string(),
    }
}

async fn test_connection(connection: &Connection, secret: &str) -> Result<ConnectionTest, String> {
    let client = client(connection, secret);
    match &connection.bucket {
        Some(bucket) => {
            client
                .head_bucket()
                .bucket(bucket)
                .send()
                .await
                .map_err(|error| bucket_error(bucket, error))?;
            Ok(ConnectionTest { buckets: None })
        }
        None => {
            let output = client.list_buckets().send().await.map_err(describe_error)?;
            let buckets = output
                .buckets()
                .iter()
                .filter_map(|bucket| bucket.name().map(str::to_string))
                .collect();
            Ok(ConnectionTest {
                buckets: Some(buckets),
            })
        }
    }
}

pub fn remote_path(id: &str, bucket: Option<&str>, prefix: Option<&str>) -> String {
    match bucket {
        Some(bucket) => format!("{SCHEME}{id}/{bucket}/{}", prefix.unwrap_or("")),
        None => format!("{SCHEME}{id}/"),
    }
}

pub fn parse_remote_path(path: &str) -> Option<RemotePath> {
    let rest = path.strip_prefix(SCHEME)?;
    let (id, rest) = rest.split_once('/').unwrap_or((rest, ""));
    if id.is_empty() {
        return None;
    }
    let (bucket, key) = rest.split_once('/').unwrap_or((rest, ""));
    Some(RemotePath {
        id: id.to_string(),
        bucket: non_empty(bucket),
        key: key.to_string(),
    })
}

async fn with_keychain<T: Send + 'static>(
    job: impl FnOnce() -> keyring::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| format!("Password store: {error}"))
}

/// Saved secret, or `None` when nothing is stored for this id.
pub(crate) async fn read_optional_secret(
    service: &'static str,
    id: String,
) -> Result<Option<String>, String> {
    with_keychain(
        move || match keyring::Entry::new(service, &id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error),
        },
    )
    .await
}

pub(crate) async fn store_secret(
    service: &'static str,
    id: String,
    secret: String,
) -> Result<(), String> {
    with_keychain(move || keyring::Entry::new(service, &id)?.set_password(&secret)).await
}

pub(crate) async fn delete_secret(service: &'static str, id: String) -> Result<(), String> {
    with_keychain(
        move || match keyring::Entry::new(service, &id)?.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            result => result,
        },
    )
    .await
}

async fn insert_connection(
    pool: &SqlitePool,
    id: &str,
    connection: &Connection,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO remote_locations (id, name, provider, endpoint, region, bucket, prefix, access_key_id, path_style, position, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT COALESCE(MAX(position), -1) + 1 FROM remote_locations), ?)",
    )
    .bind(id)
    .bind(&connection.name)
    .bind(connection.provider.as_str())
    .bind(&connection.endpoint)
    .bind(&connection.region)
    .bind(&connection.bucket)
    .bind(&connection.prefix)
    .bind(&connection.access_key_id)
    .bind(connection.path_style)
    .bind(now_secs())
    .execute(pool)
    .await?;
    Ok(())
}

/// Saved remote locations as sidebar entries, in the order they were added.
pub async fn remote_locations(pool: &SqlitePool) -> Result<Vec<Location>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, bucket, prefix FROM remote_locations ORDER BY position, name",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let id: String = row.get("id");
            let bucket: Option<String> = row.get("bucket");
            let prefix: Option<String> = row.get("prefix");
            Location {
                name: row.get("name"),
                path: remote_path(&id, bucket.as_deref(), prefix.as_deref()),
                kind: "s3".into(),
            }
        })
        .collect())
}

pub fn is_remote_path(path: &str) -> bool {
    path.starts_with(SCHEME)
}

fn bucket_error<E>(bucket: &str, error: SdkError<E, HttpResponse>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
{
    match error.raw_response().map(|raw| raw.status().as_u16()) {
        Some(404) => format!("Bucket {bucket} does not exist"),
        Some(403) => format!("Access denied to bucket {bucket}"),
        Some(301) => format!("Bucket {bucket} is in a different region"),
        _ => describe_error(error),
    }
}

fn object_error<E>(key: &str, error: SdkError<E, HttpResponse>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + 'static,
{
    match error.raw_response().map(|raw| raw.status().as_u16()) {
        Some(404) => format!("{} no longer exists", entry_name(key)),
        Some(403) => format!("Access denied to {}", entry_name(key)),
        _ => describe_error(error),
    }
}

async fn load_connection(pool: &SqlitePool, id: &str) -> Result<Connection, String> {
    let row = sqlx::query(
        "SELECT name, provider, endpoint, region, bucket, prefix, access_key_id, path_style FROM remote_locations WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .ok_or("This location was removed")?;
    let provider = match row.get::<String, _>("provider").as_str() {
        "r2" => Provider::R2,
        "custom" => Provider::Custom,
        _ => Provider::Aws,
    };
    Ok(Connection {
        provider,
        name: row.get("name"),
        endpoint: row.get("endpoint"),
        region: row.get("region"),
        bucket: row.get("bucket"),
        prefix: row.get("prefix"),
        access_key_id: row.get("access_key_id"),
        path_style: row.get("path_style"),
    })
}

async fn read_secret(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        keyring::Entry::new(KEYCHAIN_SERVICE, &id)?.get_password()
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| match error {
        keyring::Error::NoEntry => {
            "The secret for this location is missing from the keychain. Remove the location and add it again."
                .to_string()
        }
        error => format!("Keychain: {error}"),
    })
}

async fn client_for(
    pool: &SqlitePool,
    clients: &RemoteClients,
    id: &str,
) -> Result<Client, String> {
    let cell = clients
        .0
        .lock()
        .unwrap()
        .entry(id.to_string())
        .or_default()
        .clone();
    cell.get_or_try_init(|| async {
        let connection = load_connection(pool, id).await?;
        let secret = read_secret(id.to_string()).await?;
        Ok::<_, String>(client(&connection, &secret))
    })
    .await
    .cloned()
}

fn millis(time: Option<&DateTime>) -> Option<u64> {
    time.and_then(|time| time.to_millis().ok())
        .and_then(|millis| u64::try_from(millis).ok())
}

/// Last path segment of an object key or prefix: `a/b/` and `a/b` both name `b`.
fn entry_name(key: &str) -> &str {
    key.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

async fn list_buckets(client: &Client, id: &str) -> Result<Vec<DirectoryEntry>, String> {
    let output = client.list_buckets().send().await.map_err(describe_error)?;
    Ok(output
        .buckets()
        .iter()
        .filter_map(|bucket| {
            let name = bucket.name()?;
            Some(DirectoryEntry {
                name: name.to_string(),
                path: remote_path(id, Some(name), None),
                is_directory: true,
                is_hidden: false,
                size: None,
                created: millis(bucket.creation_date()),
                kind: Some("bucket"),
            })
        })
        .collect())
}

pub async fn list_directory(
    pool: &SqlitePool,
    clients: &RemoteClients,
    path: &str,
) -> Result<Vec<DirectoryEntry>, String> {
    let remote = parse_remote_path(path).ok_or("Not a remote path")?;
    let client = client_for(pool, clients, &remote.id).await?;
    let Some(bucket) = remote.bucket else {
        return list_buckets(&client, &remote.id).await;
    };
    let prefix = match remote.key.as_str() {
        "" => String::new(),
        key if key.ends_with('/') => key.to_string(),
        key => format!("{key}/"),
    };
    let base = format!("{SCHEME}{}/{bucket}/", remote.id);

    let mut entries = Vec::new();
    let mut continuation = None;
    loop {
        let output = client
            .list_objects_v2()
            .bucket(&bucket)
            .prefix(&prefix)
            .delimiter("/")
            .set_continuation_token(continuation)
            .send()
            .await
            .map_err(|error| bucket_error(&bucket, error))?;
        for common in output.common_prefixes() {
            let Some(key) = common.prefix() else { continue };
            let name = entry_name(key);
            if name.is_empty() {
                continue;
            }
            entries.push(DirectoryEntry {
                name: name.to_string(),
                path: format!("{base}{key}"),
                is_directory: true,
                is_hidden: name.starts_with('.'),
                size: None,
                created: None,
                kind: None,
            });
        }
        for object in output.contents() {
            let Some(key) = object.key() else { continue };
            // Zero-byte "folder" markers show up as keys ending in a slash.
            if key.ends_with('/') {
                continue;
            }
            let name = entry_name(key);
            entries.push(DirectoryEntry {
                name: name.to_string(),
                path: format!("{base}{key}"),
                is_directory: false,
                is_hidden: name.starts_with('.'),
                size: object.size().and_then(|size| u64::try_from(size).ok()),
                created: millis(object.last_modified()),
                kind: None,
            });
        }
        continuation = output.next_continuation_token().map(str::to_string);
        if entries.len() >= MAX_LISTING_ENTRIES
            || !output.is_truncated().unwrap_or(false)
            || continuation.is_none()
        {
            break;
        }
    }
    entries.truncate(MAX_LISTING_ENTRIES);
    Ok(entries)
}

async fn object_bytes(
    client: &Client,
    bucket: &str,
    key: &str,
    range: Option<String>,
    max_bytes: Option<i64>,
) -> Result<Vec<u8>, String> {
    let output = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .set_range(range)
        .send()
        .await
        .map_err(|error| object_error(key, error))?;
    if let (Some(max), Some(length)) = (max_bytes, output.content_length()) {
        if length > max {
            return Err(format!(
                "{} is larger than {} MB",
                entry_name(key),
                max / (1024 * 1024)
            ));
        }
    }
    let bytes = output
        .body
        .collect()
        .await
        .map_err(|error| error.to_string())?;
    Ok(bytes.into_bytes().to_vec())
}

/// Same text/binary rules as local previews: NUL in the sniffed head means binary,
/// text is cut at `PREVIEW_MAX_BYTES` on a UTF-8 boundary.
pub(crate) fn classify_preview_bytes(preview: &mut FilePreview, mut bytes: Vec<u8>) {
    if bytes[..bytes.len().min(PREVIEW_SNIFF_BYTES)].contains(&0) {
        preview.kind = PreviewKind::Binary;
        return;
    }
    if bytes.len() > PREVIEW_MAX_BYTES {
        bytes.truncate(utf8_boundary(&bytes, PREVIEW_MAX_BYTES));
        preview.truncated = true;
    }
    preview.kind = PreviewKind::Text;
    preview.content = Some(String::from_utf8_lossy(&bytes).into_owned());
}

pub async fn file_preview(
    pool: &SqlitePool,
    clients: &RemoteClients,
    path: &str,
) -> Result<FilePreview, String> {
    let remote = parse_remote_path(path).ok_or("Not a remote path")?;
    let mut preview = FilePreview {
        name: String::new(),
        size: 0,
        created: None,
        modified: None,
        kind: PreviewKind::Directory,
        content: None,
        src: None,
        truncated: false,
    };
    let Some(bucket) = remote.bucket else {
        preview.name = load_connection(pool, &remote.id).await?.name;
        return Ok(preview);
    };
    if remote.key.is_empty() || remote.key.ends_with('/') {
        preview.name = match entry_name(&remote.key) {
            "" => bucket,
            name => name.to_string(),
        };
        return Ok(preview);
    }

    let key = remote.key;
    let client = client_for(pool, clients, &remote.id).await?;
    let head = client
        .head_object()
        .bucket(&bucket)
        .key(&key)
        .send()
        .await
        .map_err(|error| object_error(&key, error))?;
    preview.name = entry_name(&key).to_string();
    preview.size = head
        .content_length()
        .and_then(|size| u64::try_from(size).ok())
        .unwrap_or(0);
    preview.modified = millis(head.last_modified());

    let extension = Path::new(&key)
        .extension()
        .and_then(|extension| extension.to_str());
    if let Some(mime) = extension.and_then(image_mime) {
        if preview.size >= IMAGE_MAX_BYTES as u64 {
            return Err(format!(
                "Image exceeds {} MB preview limit",
                IMAGE_MAX_BYTES / (1024 * 1024)
            ));
        }
        let bytes = object_bytes(&client, &bucket, &key, None, None).await?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        preview.kind = PreviewKind::Image;
        preview.src = Some(format!("data:{mime};base64,{encoded}"));
        return Ok(preview);
    }

    if extension.is_some_and(|extension| extension.eq_ignore_ascii_case("pdf")) {
        if preview.size > PDF_MAX_BYTES as u64 {
            return Err(format!(
                "PDF exceeds {} MB preview limit",
                PDF_MAX_BYTES / (1024 * 1024)
            ));
        }
        let bytes = object_bytes(&client, &bucket, &key, None, None).await?;
        set_pdf_preview(&mut preview, bytes);
        return Ok(preview);
    }

    if preview.size == 0 {
        classify_preview_bytes(&mut preview, Vec::new());
        return Ok(preview);
    }
    // One byte past the limit tells a truncated file from one that fits exactly.
    let last = preview.size.min(PREVIEW_MAX_BYTES as u64 + 1) - 1;
    let bytes = object_bytes(
        &client,
        &bucket,
        &key,
        Some(format!("bytes=0-{last}")),
        None,
    )
    .await?;
    classify_preview_bytes(&mut preview, bytes);
    Ok(preview)
}

/// Local cache file for an object. Empty, `.` and `..` segments are dropped so keys
/// can't escape the cache directory.
fn cache_file(root: &Path, id: &str, bucket: &str, key: &str) -> Option<PathBuf> {
    let valid = |part: &str| !part.is_empty() && part != "." && part != ".." && !part.contains('/');
    if !valid(id) || !valid(bucket) {
        return None;
    }
    write::safe_join(&root.join("remote").join(id).join(bucket), key)
}

/// Downloads an object into the app cache and returns the local file path, so the
/// frontend can open it with the default app. Edits to the copy are not uploaded.
#[tauri::command]
pub async fn download_remote_file(
    app: AppHandle,
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    sessions: State<'_, crate::network::servers::Sessions>,
    path: String,
) -> Result<String, String> {
    if crate::network::servers::is_server_path(&path) {
        return crate::network::servers::download_to_cache(&app, &database.0, &sessions, &path)
            .await;
    }
    let remote = parse_remote_path(&path).ok_or("Not a remote path")?;
    let bucket = remote.bucket.ok_or("Buckets can't be opened as files")?;
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?;
    let target = cache_file(&root, &remote.id, &bucket, &remote.key)
        .filter(|_| !remote.key.ends_with('/'))
        .ok_or("Folders can't be opened as files")?;
    let client = client_for(&database.0, &clients, &remote.id).await?;
    write::download_to(
        &client,
        &bucket,
        &remote.key,
        &target,
        Some(DOWNLOAD_MAX_BYTES),
        |_, _| {},
    )
    .await?;
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn test_remote_location(input: RemoteLocationInput) -> Result<ConnectionTest, String> {
    let connection = resolve(&input)?;
    test_connection(&connection, &input.secret_access_key).await
}

#[tauri::command]
pub async fn add_remote_location(
    database: State<'_, Database>,
    input: RemoteLocationInput,
) -> Result<Location, String> {
    let connection = resolve(&input)?;
    test_connection(&connection, &input.secret_access_key).await?;

    let id = uuid::Uuid::new_v4().to_string();
    store_secret(
        KEYCHAIN_SERVICE,
        id.clone(),
        input.secret_access_key.clone(),
    )
    .await?;
    if let Err(error) = insert_connection(&database.0, &id, &connection).await {
        let _ = delete_secret(KEYCHAIN_SERVICE, id).await;
        return Err(error.to_string());
    }

    Ok(Location {
        name: connection.name,
        path: remote_path(
            &id,
            connection.bucket.as_deref(),
            connection.prefix.as_deref(),
        ),
        kind: "s3".into(),
    })
}

#[tauri::command]
pub async fn remove_remote_location(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    path: String,
) -> Result<(), String> {
    let remote = parse_remote_path(&path).ok_or("Not a remote location")?;
    clients.0.lock().unwrap().remove(&remote.id);
    sqlx::query("DELETE FROM remote_locations WHERE id = ?")
        .bind(&remote.id)
        .execute(&database.0)
        .await
        .map_err(|error| error.to_string())?;
    delete_secret(KEYCHAIN_SERVICE, remote.id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    fn input(provider: Provider) -> RemoteLocationInput {
        RemoteLocationInput {
            provider,
            name: String::new(),
            account_id: String::new(),
            endpoint: String::new(),
            region: String::new(),
            access_key_id: " AKIAEXAMPLE ".into(),
            secret_access_key: "secret".into(),
            bucket: String::new(),
            prefix: String::new(),
            path_style: true,
        }
    }

    #[test]
    fn aws_defaults_region_and_virtual_hosted_style() {
        let connection = resolve(&input(Provider::Aws)).unwrap();
        assert_eq!(connection.region, "us-east-1");
        assert_eq!(connection.endpoint, None);
        assert!(!connection.path_style);
        assert_eq!(connection.access_key_id, "AKIAEXAMPLE");
        assert_eq!(connection.name, "Amazon S3");
    }

    #[test]
    fn r2_derives_endpoint_from_account_id() {
        let mut r2 = input(Provider::R2);
        r2.account_id = "3F2A9C0B1D4E5F60718293A4B5C6D7E8".into();
        r2.bucket = "media".into();
        let connection = resolve(&r2).unwrap();
        assert_eq!(
            connection.endpoint.as_deref(),
            Some("https://3f2a9c0b1d4e5f60718293a4b5c6d7e8.r2.cloudflarestorage.com")
        );
        assert_eq!(connection.region, "auto");
        assert_eq!(connection.name, "media");
    }

    #[test]
    fn r2_rejects_malformed_account_id() {
        let mut r2 = input(Provider::R2);
        r2.account_id = "not-an-account".into();
        assert!(resolve(&r2).is_err());
    }

    #[test]
    fn custom_requires_http_endpoint_and_trims_slash() {
        let mut custom = input(Provider::Custom);
        assert!(resolve(&custom).is_err());
        custom.endpoint = "minio.local:9000".into();
        assert!(resolve(&custom).is_err());
        custom.endpoint = "http://minio.local:9000/".into();
        let connection = resolve(&custom).unwrap();
        assert_eq!(
            connection.endpoint.as_deref(),
            Some("http://minio.local:9000")
        );
        assert!(connection.path_style);
    }

    #[test]
    fn missing_credentials_are_rejected() {
        let mut aws = input(Provider::Aws);
        aws.secret_access_key.clear();
        assert!(resolve(&aws).is_err());
        let mut aws = input(Provider::Aws);
        aws.access_key_id = "  ".into();
        assert!(resolve(&aws).is_err());
    }

    #[test]
    fn prefix_is_normalized_and_requires_bucket() {
        let mut aws = input(Provider::Aws);
        aws.prefix = "/uploads/2026".into();
        assert_eq!(resolve(&aws).unwrap().prefix, None);
        aws.bucket = "assets".into();
        assert_eq!(
            resolve(&aws).unwrap().prefix.as_deref(),
            Some("uploads/2026/")
        );
    }

    #[test]
    fn remote_paths_round_trip() {
        assert_eq!(remote_path("abc", None, None), "s3://abc/");
        assert_eq!(
            parse_remote_path("s3://abc/"),
            Some(RemotePath {
                id: "abc".into(),
                bucket: None,
                key: String::new()
            })
        );
        let path = remote_path("abc", Some("assets"), Some("uploads/"));
        assert_eq!(path, "s3://abc/assets/uploads/");
        assert_eq!(
            parse_remote_path(&path),
            Some(RemotePath {
                id: "abc".into(),
                bucket: Some("assets".into()),
                key: "uploads/".into()
            })
        );
        assert_eq!(parse_remote_path("/Users/me"), None);
        assert_eq!(parse_remote_path("s3:///"), None);
    }

    #[test]
    fn entry_names_come_from_the_last_segment() {
        assert_eq!(entry_name("photos/2026/"), "2026");
        assert_eq!(entry_name("photos/2026/cat.png"), "cat.png");
        assert_eq!(entry_name("readme.md"), "readme.md");
        assert_eq!(entry_name(""), "");
        assert!(is_remote_path("s3://abc/"));
        assert!(!is_remote_path("/Users/me"));
    }

    #[test]
    fn cache_file_stays_inside_the_cache_directory() {
        let root = Path::new("/cache");
        assert_eq!(
            cache_file(root, "id", "bucket", "a/../../b//c.txt"),
            Some(PathBuf::from("/cache/remote/id/bucket/a/b/c.txt"))
        );
        assert_eq!(cache_file(root, "id", "bucket", "../.."), None);
        assert_eq!(cache_file(root, "..", "bucket", "c.txt"), None);
    }

    fn empty_preview() -> FilePreview {
        FilePreview {
            name: "x".into(),
            size: 0,
            created: None,
            modified: None,
            kind: PreviewKind::Directory,
            content: None,
            src: None,
            truncated: false,
        }
    }

    #[test]
    fn preview_bytes_follow_local_text_rules() {
        let mut text = empty_preview();
        classify_preview_bytes(&mut text, b"hello".to_vec());
        assert_eq!(text.kind, PreviewKind::Text);
        assert_eq!(text.content.as_deref(), Some("hello"));
        assert!(!text.truncated);

        let mut binary = empty_preview();
        classify_preview_bytes(&mut binary, b"\x89PNG\x00".to_vec());
        assert_eq!(binary.kind, PreviewKind::Binary);

        let mut long = empty_preview();
        classify_preview_bytes(&mut long, vec![b'a'; PREVIEW_MAX_BYTES + 1]);
        assert!(long.truncated);
        assert_eq!(
            long.content.map(|content| content.len()),
            Some(PREVIEW_MAX_BYTES)
        );
    }

    #[test]
    fn saved_connections_are_listed_as_s3_locations() {
        tauri::async_runtime::block_on(async {
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            super::super::apply_migrations(&pool).await.unwrap();

            let mut aws = input(Provider::Aws);
            aws.bucket = "assets".into();
            insert_connection(&pool, "one", &resolve(&aws).unwrap())
                .await
                .unwrap();
            insert_connection(&pool, "two", &resolve(&input(Provider::Aws)).unwrap())
                .await
                .unwrap();

            let locations = remote_locations(&pool).await.unwrap();
            let paths: Vec<_> = locations.iter().map(|l| l.path.as_str()).collect();
            assert_eq!(paths, ["s3://one/assets/", "s3://two/"]);
            assert!(locations.iter().all(|location| location.kind == "s3"));
        });
    }
}
