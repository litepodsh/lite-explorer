//! Write operations on remote storage: create, rename, copy, delete, upload and download.
//!
//! S3 has no real folders or renames. A folder is every key under a `prefix/`, sometimes
//! plus a zero-byte `prefix/` marker. Renaming copies each key and deletes the originals.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use aws_sdk_s3::{
    primitives::ByteStream,
    types::{Delete, ObjectIdentifier},
    Client,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use sqlx::SqlitePool;
use tauri::{AppHandle, State};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::{
    bucket_error, client_for, entry_name, object_error, parse_remote_path, RemoteClients, SCHEME,
};
use crate::transfer::{TransferEvent, TransferRegistry};
use crate::{image_mime, transfer, unique_name, Database, DirectoryEntry};

/// S3 rejects single PUT and CopyObject requests above 5 GiB.
const SINGLE_REQUEST_MAX_BYTES: u64 = 5 * 1024 * 1024 * 1024;
const DELETE_BATCH: usize = 1000;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

/// Characters left as-is in a `CopySource` header; everything else is percent-encoded.
const COPY_SOURCE_KEEP: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

/// A remote path that points inside a bucket.
#[derive(Debug, PartialEq)]
struct Target {
    id: String,
    bucket: String,
    key: String,
}

fn target(path: &str) -> Result<Target, String> {
    let remote = parse_remote_path(path).ok_or("Not a remote path")?;
    let bucket = remote.bucket.ok_or("Open a bucket first")?;
    Ok(Target {
        id: remote.id,
        bucket,
        key: remote.key,
    })
}

fn is_folder(key: &str) -> bool {
    key.is_empty() || key.ends_with('/')
}

fn folder_prefix(key: &str) -> String {
    if is_folder(key) {
        key.to_string()
    } else {
        format!("{key}/")
    }
}

/// Prefix of the folder that contains `key`: `a/b/` and `a/b.txt` both live in `a/`.
fn parent_prefix(key: &str) -> &str {
    let trimmed = key.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(index) => &key[..=index],
        None => "",
    }
}

fn child_key(prefix: &str, name: &str, folder: bool) -> String {
    if folder {
        format!("{prefix}{name}/")
    } else {
        format!("{prefix}{name}")
    }
}

/// Where `key` lands when the folder `from` is copied to `to`.
fn relocated_key(from: &str, to: &str, key: &str) -> String {
    format!("{to}{}", &key[from.len()..])
}

fn valid_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name must not be empty".into());
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err("Name can’t contain “/” or be “.” or “..”".into());
    }
    Ok(name)
}

fn copy_source(bucket: &str, key: &str) -> String {
    format!("{bucket}/{}", utf8_percent_encode(key, COPY_SOURCE_KEEP))
}

/// Joins a slash-separated object key under `base`, dropping empty, `.` and `..`
/// segments so the result can't escape `base`. `None` when nothing is left.
pub(super) fn safe_join(base: &Path, key: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = key
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut path = base.to_path_buf();
    path.extend(parts);
    Some(path)
}

fn content_type(name: &str) -> Option<&'static str> {
    let extension = Path::new(name).extension()?.to_str()?;
    image_mime(extension).or(match extension.to_ascii_lowercase().as_str() {
        "txt" | "md" | "log" | "csv" => Some("text/plain; charset=utf-8"),
        "html" | "htm" => Some("text/html; charset=utf-8"),
        "css" => Some("text/css; charset=utf-8"),
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "json" => Some("application/json"),
        "pdf" => Some("application/pdf"),
        "zip" => Some("application/zip"),
        "mp4" => Some("video/mp4"),
        "mp3" => Some("audio/mpeg"),
        _ => None,
    })
}

fn remote_entry(target: &Target, key: &str, size: Option<u64>) -> DirectoryEntry {
    let name = entry_name(key);
    DirectoryEntry {
        name: name.to_string(),
        path: format!("{SCHEME}{}/{}/{key}", target.id, target.bucket),
        is_directory: is_folder(key),
        is_hidden: name.starts_with('.'),
        size,
        created: None,
        kind: None,
    }
}

async fn key_exists(client: &Client, bucket: &str, key: &str) -> Result<bool, String> {
    if is_folder(key) {
        let output = client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(key)
            .max_keys(1)
            .send()
            .await
            .map_err(|error| bucket_error(bucket, error))?;
        return Ok(!output.contents().is_empty());
    }
    match client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Ok(true),
        Err(error)
            if error
                .raw_response()
                .is_some_and(|raw| raw.status().as_u16() == 404) =>
        {
            Ok(false)
        }
        Err(error) => Err(object_error(key, error)),
    }
}

/// A name is taken when either a file or a folder with that name exists.
async fn name_taken(
    client: &Client,
    bucket: &str,
    prefix: &str,
    name: &str,
) -> Result<bool, String> {
    Ok(
        key_exists(client, bucket, &child_key(prefix, name, false)).await?
            || key_exists(client, bucket, &child_key(prefix, name, true)).await?,
    )
}

/// Key for `base` in `prefix`, numbered like local files ("name 2") when taken.
async fn unique_child(
    client: &Client,
    bucket: &str,
    prefix: &str,
    base: &str,
    folder: bool,
) -> Result<String, String> {
    let mut name = base.to_string();
    let mut index = 2;
    while name_taken(client, bucket, prefix, &name).await? {
        name = format!("{base} {index}");
        index += 1;
    }
    Ok(child_key(prefix, &name, folder))
}

/// Every key under `prefix` with its size, following continuation tokens.
async fn list_keys(
    client: &Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<(String, u64)>, String> {
    let mut keys = Vec::new();
    let mut continuation = None;
    loop {
        let output = client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .set_continuation_token(continuation)
            .send()
            .await
            .map_err(|error| bucket_error(bucket, error))?;
        keys.extend(output.contents().iter().filter_map(|object| {
            let size = object.size().and_then(|size| u64::try_from(size).ok());
            Some((object.key()?.to_string(), size.unwrap_or(0)))
        }));
        continuation = output.next_continuation_token().map(str::to_string);
        if !output.is_truncated().unwrap_or(false) || continuation.is_none() {
            return Ok(keys);
        }
    }
}

async fn copy_object(client: &Client, from: (&str, &str), to: (&str, &str)) -> Result<(), String> {
    client
        .copy_object()
        .bucket(to.0)
        .key(to.1)
        .copy_source(copy_source(from.0, from.1))
        .send()
        .await
        .map_err(|error| object_error(from.1, error))?;
    Ok(())
}

async fn delete_keys(client: &Client, bucket: &str, keys: &[String]) -> Result<(), String> {
    for chunk in keys.chunks(DELETE_BATCH) {
        let objects = chunk
            .iter()
            .map(|key| ObjectIdentifier::builder().key(key).build())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        let delete = Delete::builder()
            .set_objects(Some(objects))
            .quiet(true)
            .build()
            .map_err(|error| error.to_string())?;
        match client
            .delete_objects()
            .bucket(bucket)
            .delete(delete)
            .send()
            .await
        {
            Ok(output) => {
                if let Some(error) = output.errors().first() {
                    return Err(format!(
                        "Couldn’t delete {}: {}",
                        entry_name(error.key().unwrap_or_default()),
                        error.message().unwrap_or("unknown error")
                    ));
                }
            }
            // Some S3-compatible services don't implement DeleteObjects. Single deletes
            // also surface the real error when the batch failed for another reason.
            Err(_) => {
                for key in chunk {
                    client
                        .delete_object()
                        .bucket(bucket)
                        .key(key)
                        .send()
                        .await
                        .map_err(|error| object_error(key, error))?;
                }
            }
        }
    }
    Ok(())
}

/// Copies the object or folder `from` to `to`, then deletes the source when `move_source`.
async fn relocate(
    client: &Client,
    from: (&str, &str),
    to: (&str, &str),
    move_source: bool,
) -> Result<(), String> {
    let keys = if is_folder(from.1) {
        list_keys(client, from.0, from.1).await?
    } else {
        vec![(from.1.to_string(), 0)]
    };
    for (key, size) in &keys {
        if *size > SINGLE_REQUEST_MAX_BYTES {
            return Err(format!(
                "{} is larger than 5 GB, which can’t be copied yet",
                entry_name(key)
            ));
        }
        let destination = if is_folder(from.1) {
            relocated_key(from.1, to.1, key)
        } else {
            to.1.to_string()
        };
        copy_object(client, (from.0, key), (to.0, &destination)).await?;
    }
    if move_source {
        let keys: Vec<String> = keys.into_iter().map(|(key, _)| key).collect();
        delete_keys(client, from.0, &keys).await?;
    }
    Ok(())
}

pub async fn create_item(
    pool: &SqlitePool,
    clients: &RemoteClients,
    parent: &str,
    kind: &str,
    name: &str,
) -> Result<DirectoryEntry, String> {
    let parent = target(parent)?;
    let prefix = folder_prefix(&parent.key);
    let name = valid_name(name)?;
    let folder = kind == "folder";
    let client = client_for(pool, clients, &parent.id).await?;
    let key = unique_child(&client, &parent.bucket, &prefix, name, folder).await?;
    let mut request = client
        .put_object()
        .bucket(&parent.bucket)
        .key(&key)
        .body(ByteStream::from_static(b""));
    if let Some(content_type) = content_type(&key) {
        request = request.content_type(content_type);
    }
    request
        .send()
        .await
        .map_err(|error| object_error(&key, error))?;
    Ok(remote_entry(&parent, &key, (!folder).then_some(0)))
}

pub async fn rename_item(
    pool: &SqlitePool,
    clients: &RemoteClients,
    path: &str,
    new_name: &str,
) -> Result<DirectoryEntry, String> {
    let source = target(path)?;
    if source.key.is_empty() {
        return Err("Buckets can’t be renamed".into());
    }
    let name = valid_name(new_name)?;
    let folder = is_folder(&source.key);
    let prefix = parent_prefix(&source.key);
    let renamed = child_key(prefix, name, folder);
    if renamed == source.key {
        return Ok(remote_entry(&source, &renamed, None));
    }
    let client = client_for(pool, clients, &source.id).await?;
    if name_taken(&client, &source.bucket, prefix, name).await? {
        return Err(format!("“{name}” already exists"));
    }
    relocate(
        &client,
        (&source.bucket, &source.key),
        (&source.bucket, &renamed),
        true,
    )
    .await?;
    Ok(remote_entry(&source, &renamed, None))
}

/// Copies `path` into the folder `destination` of the same location, choosing a
/// numbered name on collision. Duplicate passes the item's own parent folder.
pub async fn copy_item(
    pool: &SqlitePool,
    clients: &RemoteClients,
    path: &str,
    destination: &str,
) -> Result<DirectoryEntry, String> {
    let (Some(source), Some(folder)) = (
        target(path).ok().filter(|source| !source.key.is_empty()),
        target(destination).ok(),
    ) else {
        return Err("Copying between local and remote storage isn’t supported yet".into());
    };
    if source.id != folder.id {
        return Err("Copying between storage locations isn’t supported yet".into());
    }
    let prefix = folder_prefix(&folder.key);
    let is_source_folder = is_folder(&source.key);
    if is_source_folder && source.bucket == folder.bucket && prefix.starts_with(&source.key) {
        return Err("A folder can’t be copied into itself".into());
    }
    let client = client_for(pool, clients, &source.id).await?;
    let key = unique_child(
        &client,
        &folder.bucket,
        &prefix,
        entry_name(&source.key),
        is_source_folder,
    )
    .await?;
    relocate(
        &client,
        (&source.bucket, &source.key),
        (&folder.bucket, &key),
        false,
    )
    .await?;
    Ok(remote_entry(&folder, &key, None))
}

#[tauri::command]
pub async fn delete_remote_items(
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    sessions: State<'_, crate::network::servers::Sessions>,
    paths: Vec<String>,
) -> Result<(), String> {
    if paths
        .iter()
        .any(|path| crate::network::servers::is_server_path(path))
    {
        return crate::network::servers::delete_items(&database.0, &sessions, &paths).await;
    }
    for path in paths {
        let item = target(&path)?;
        if item.key.is_empty() {
            return Err("Buckets can’t be deleted here".into());
        }
        let client = client_for(&database.0, &clients, &item.id).await?;
        let keys = if is_folder(&item.key) {
            list_keys(&client, &item.bucket, &item.key)
                .await?
                .into_iter()
                .map(|(key, _)| key)
                .collect()
        } else {
            vec![item.key]
        };
        delete_keys(&client, &item.bucket, &keys).await?;
    }
    Ok(())
}

/// Emits `transfer-progress` events, throttled while bytes flow, and stops when cancelled.
struct Progress {
    app: AppHandle,
    token: CancellationToken,
    state: TransferEvent,
    last_emit: Instant,
}

impl Progress {
    #[allow(clippy::too_many_arguments)]
    fn new(
        app: AppHandle,
        token: CancellationToken,
        id: String,
        kind: &str,
        destination: String,
        files_total: u64,
        bytes_total: u64,
    ) -> Self {
        let progress = Self {
            app,
            token,
            state: TransferEvent::new(id, kind, destination, files_total, bytes_total),
            last_emit: Instant::now(),
        };
        progress.emit();
        progress
    }

    fn cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    fn emit(&self) {
        transfer::emit(&self.app, &self.state);
    }

    fn start_file(&mut self, name: &str) {
        self.state.label = name.to_string();
        self.emit();
        self.last_emit = Instant::now();
    }

    fn add_bytes(&mut self, bytes: u64) {
        self.state.add_bytes(bytes);
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.emit();
            self.last_emit = Instant::now();
        }
    }

    fn finish_file(&mut self) {
        self.state.files_done += 1;
        self.state.file_progress = None;
        self.emit();
    }

    fn finish(mut self) {
        self.state.state = if self.token.is_cancelled() {
            "cancelled".to_string()
        } else {
            "done".to_string()
        };
        self.state.label.clear();
        self.emit();
    }

    fn fail(mut self, error: String) {
        self.state.state = "failed".to_string();
        self.state.error = Some(error);
        self.emit();
    }
}

/// One upload: a local file, or `None` for the marker of an empty folder.
#[derive(Debug, PartialEq)]
struct UploadItem {
    local: Option<PathBuf>,
    key: String,
    size: u64,
}

/// Collects files under `dir` as keys below `prefix` (which ends in `/`). Empty folders
/// become folder markers so they survive the upload. `.DS_Store` files are skipped.
fn walk_local(dir: &Path, prefix: &str, items: &mut Vec<UploadItem>) -> std::io::Result<()> {
    let mut children: Vec<_> = fs::read_dir(dir)?.filter_map(Result::ok).collect();
    children.sort_by_key(|child| child.file_name());
    let before = items.len();
    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        if name == ".DS_Store" {
            continue;
        }
        let metadata = fs::metadata(child.path())?;
        if metadata.is_dir() {
            walk_local(&child.path(), &format!("{prefix}{name}/"), items)?;
        } else {
            items.push(UploadItem {
                local: Some(child.path()),
                key: format!("{prefix}{name}"),
                size: metadata.len(),
            });
        }
    }
    if items.len() == before {
        items.push(UploadItem {
            local: None,
            key: prefix.to_string(),
            size: 0,
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn upload_remote_files(
    app: AppHandle,
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    transfers: State<'_, TransferRegistry>,
    sessions: State<'_, crate::network::servers::Sessions>,
    destination: String,
    sources: Vec<String>,
) -> Result<(), String> {
    if crate::network::servers::is_server_path(&destination) {
        return crate::network::servers::upload_files(
            app,
            &database.0,
            &sessions,
            &transfers,
            destination,
            sources,
        )
        .await;
    }
    let folder = target(&destination)?;
    let prefix = folder_prefix(&folder.key);
    let client = client_for(&database.0, &clients, &folder.id).await?;

    let mut items = Vec::new();
    for source in sources {
        let local = PathBuf::from(&source);
        let metadata = fs::metadata(&local).map_err(|error| format!("{source}: {error}"))?;
        let base = local
            .file_name()
            .ok_or("Invalid file to upload")?
            .to_string_lossy()
            .into_owned();
        let key = unique_child(&client, &folder.bucket, &prefix, &base, metadata.is_dir()).await?;
        if metadata.is_dir() {
            let found = tauri::async_runtime::spawn_blocking(move || {
                let mut found = Vec::new();
                walk_local(&local, &key, &mut found).map(|_| found)
            })
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
            items.extend(found);
        } else {
            items.push(UploadItem {
                local: Some(local),
                key,
                size: metadata.len(),
            });
        }
    }
    if let Some(item) = items
        .iter()
        .find(|item| item.size > SINGLE_REQUEST_MAX_BYTES)
    {
        return Err(format!(
            "{} is larger than 5 GB, which can’t be uploaded yet",
            entry_name(&item.key)
        ));
    }

    let total_bytes = items.iter().map(|item| item.size).sum();
    let id = uuid::Uuid::new_v4().to_string();
    let token = transfers.register(&id);
    let mut progress = Progress::new(
        app,
        token,
        id.clone(),
        "upload",
        destination,
        items.len() as u64,
        total_bytes,
    );
    let result = async {
        for item in &items {
            if progress.cancelled() {
                break;
            }
            progress.start_file(entry_name(&item.key));
            let body = match &item.local {
                Some(path) => ByteStream::from_path(path)
                    .await
                    .map_err(|error| error.to_string())?,
                None => ByteStream::from_static(b""),
            };
            let mut request = client
                .put_object()
                .bucket(&folder.bucket)
                .key(&item.key)
                .content_length(item.size as i64)
                .body(body);
            if let Some(content_type) = content_type(&item.key) {
                request = request.content_type(content_type);
            }
            request
                .send()
                .await
                .map_err(|error| object_error(&item.key, error))?;
            progress.add_bytes(item.size);
            progress.finish_file();
        }
        Ok::<_, String>(())
    }
    .await;
    match &result {
        Ok(()) => progress.finish(),
        Err(error) => progress.fail(error.clone()),
    }
    transfers.remove(&id);
    result
}

/// Streams an object into `local`, creating parent folders. A partial file is removed on error.
pub(super) async fn download_to(
    client: &Client,
    bucket: &str,
    key: &str,
    local: &Path,
    max_bytes: Option<u64>,
    mut on_bytes: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let mut output = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|error| object_error(key, error))?;
    if let (Some(max), Some(length)) = (max_bytes, output.content_length()) {
        if u64::try_from(length).unwrap_or(0) > max {
            return Err(format!(
                "{} is larger than {} MB",
                entry_name(key),
                max / (1024 * 1024)
            ));
        }
    }
    let total = output
        .content_length()
        .and_then(|length| u64::try_from(length).ok());
    on_bytes(0, total);
    if let Some(parent) = local.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
    }
    let result = async {
        let mut file = tokio::fs::File::create(local).await?;
        while let Some(chunk) = output
            .body
            .try_next()
            .await
            .map_err(std::io::Error::other)?
        {
            file.write_all(&chunk).await?;
            on_bytes(chunk.len() as u64, total);
        }
        file.flush().await
    }
    .await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(local).await;
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn download_remote_items(
    app: AppHandle,
    database: State<'_, Database>,
    clients: State<'_, RemoteClients>,
    transfers: State<'_, TransferRegistry>,
    sessions: State<'_, crate::network::servers::Sessions>,
    paths: Vec<String>,
    destination: String,
) -> Result<(), String> {
    if paths
        .iter()
        .any(|path| crate::network::servers::is_server_path(path))
    {
        return crate::network::servers::download_items(
            app,
            &database.0,
            &sessions,
            &transfers,
            paths,
            destination,
        )
        .await;
    }
    let destination_label = destination.clone();
    let destination = PathBuf::from(destination);
    if !destination.is_dir() {
        return Err(format!("{} is not a folder", destination.display()));
    }
    let first = target(paths.first().ok_or("Nothing to download")?)?;
    let client = client_for(&database.0, &clients, &first.id).await?;

    // (bucket, key, local file, size)
    let mut files: Vec<(String, String, PathBuf, u64)> = Vec::new();
    for path in &paths {
        let item = target(path)?;
        if item.id != first.id {
            return Err("Download items from one location at a time".into());
        }
        let name = match entry_name(&item.key) {
            "" => item.bucket.clone(),
            name => name.to_string(),
        };
        let top = destination.join(unique_name(&destination, &name));
        if !is_folder(&item.key) {
            files.push((item.bucket, item.key, top, 0));
            continue;
        }
        fs::create_dir_all(&top).map_err(|error| error.to_string())?;
        for (key, size) in list_keys(&client, &item.bucket, &item.key).await? {
            let relative = &key[item.key.len()..];
            let Some(local) = safe_join(&top, relative) else {
                continue;
            };
            if key.ends_with('/') {
                fs::create_dir_all(&local).map_err(|error| error.to_string())?;
            } else {
                files.push((item.bucket.clone(), key, local, size));
            }
        }
    }

    let total_bytes = files.iter().map(|file| file.3).sum();
    let id = uuid::Uuid::new_v4().to_string();
    let token = transfers.register(&id);
    let mut progress = Progress::new(
        app,
        token,
        id.clone(),
        "download",
        destination_label,
        files.len() as u64,
        total_bytes,
    );
    let result = async {
        for (bucket, key, local, size) in &files {
            if progress.cancelled() {
                break;
            }
            progress.state.start_download(local, *size);
            progress.start_file(entry_name(key));
            download_to(&client, bucket, key, local, None, |bytes, total| {
                if let (Some(file), Some(total)) = (&mut progress.state.file_progress, total) {
                    progress.state.bytes_total =
                        progress.state.bytes_total.saturating_sub(file.bytes_total) + total;
                    file.bytes_total = total;
                }
                progress.add_bytes(bytes)
            })
            .await?;
            progress.finish_file();
        }
        Ok::<_, String>(())
    }
    .await;
    match &result {
        Ok(()) => progress.finish(),
        Err(error) => progress.fail(error.clone()),
    }
    transfers.remove(&id);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_prefixes() {
        assert_eq!(parent_prefix("a/b/"), "a/");
        assert_eq!(parent_prefix("a/b.txt"), "a/");
        assert_eq!(parent_prefix("b.txt"), "");
        assert_eq!(parent_prefix("b/"), "");
        assert_eq!(folder_prefix(""), "");
        assert_eq!(folder_prefix("a"), "a/");
        assert_eq!(child_key("a/", "b", true), "a/b/");
        assert_eq!(child_key("", "b.txt", false), "b.txt");
        assert_eq!(
            relocated_key("a/old/", "a/new/", "a/old/x/y.txt"),
            "a/new/x/y.txt"
        );
        assert_eq!(relocated_key("a/old/", "a/new/", "a/old/"), "a/new/");
    }

    #[test]
    fn names_are_validated() {
        assert_eq!(valid_name("  report.pdf "), Ok("report.pdf"));
        assert!(valid_name("").is_err());
        assert!(valid_name("a/b").is_err());
        assert!(valid_name("..").is_err());
    }

    #[test]
    fn copy_source_is_percent_encoded() {
        assert_eq!(
            copy_source("media", "photos/a b+c.png"),
            "media/photos/a%20b%2Bc.png"
        );
        assert_eq!(
            copy_source("media", "ñ/x~y_z-1.txt"),
            "media/%C3%B1/x~y_z-1.txt"
        );
    }

    #[test]
    fn remote_targets_require_a_bucket() {
        assert!(target("s3://id/").is_err());
        assert_eq!(
            target("s3://id/media/a/b.txt"),
            Ok(Target {
                id: "id".into(),
                bucket: "media".into(),
                key: "a/b.txt".into()
            })
        );
    }

    #[test]
    fn safe_join_stays_under_base() {
        let base = Path::new("/tmp/dl");
        assert_eq!(
            safe_join(base, "x/../y.txt"),
            Some(PathBuf::from("/tmp/dl/x/y.txt"))
        );
        assert_eq!(safe_join(base, "../.."), None);
        assert_eq!(safe_join(base, ""), None);
    }

    #[test]
    fn content_types_cover_common_files() {
        assert_eq!(content_type("a/cat.PNG"), Some("image/png"));
        assert_eq!(content_type("notes.md"), Some("text/plain; charset=utf-8"));
        assert_eq!(content_type("folder/"), None);
    }

    #[test]
    fn walking_a_folder_keeps_structure_and_empty_folders() {
        let root = std::env::temp_dir().join(format!("liteexplorer-upload-{}", crate::now_secs()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/empty")).unwrap();
        fs::write(root.join("docs/a.txt"), "hello").unwrap();
        fs::write(root.join(".DS_Store"), "x").unwrap();

        let mut items = Vec::new();
        walk_local(&root, "up/", &mut items).unwrap();
        let keys: Vec<_> = items
            .iter()
            .map(|item| (item.key.as_str(), item.size))
            .collect();
        assert_eq!(keys, [("up/docs/a.txt", 5), ("up/docs/empty/", 0)]);

        let empty = root.join("docs/empty");
        let mut only_marker = Vec::new();
        walk_local(&empty, "e/", &mut only_marker).unwrap();
        assert_eq!(only_marker[0].local, None);
        fs::remove_dir_all(&root).unwrap();
    }
}
