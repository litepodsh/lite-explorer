//! Local archive creation, listing and extraction: `.zip`, `.tar`, `.tar.gz` / `.tgz`.
//!
//! Extraction writes each top-level target to a hidden staging path and renames it into
//! place once the whole archive has been read. Entries that escape the destination and
//! links are never extracted.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Zip,
    Tar,
    TarGz,
}

fn format_for(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".zip") {
        Some(Format::Zip)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Format::TarGz)
    } else if name.ends_with(".tar") {
        Some(Format::Tar)
    } else {
        None
    }
}

const UNSUPPORTED: &str = "Unsupported format. Use .zip, .tar, .tar.gz or .tgz";

/// Entries listed for a preview. Extraction always reads the whole archive.
pub const LIST_LIMIT: usize = 100_000;

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn is_archive_path(path: &Path) -> bool {
    format_for(path).is_some()
}

pub fn create(paths: &[PathBuf], destination: &Path) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing to compress".into());
    }
    for path in paths {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }
    match format_for(destination).ok_or(UNSUPPORTED)? {
        Format::Zip => create_zip(paths, destination),
        Format::Tar => create_tar(paths, destination, false),
        Format::TarGz => create_tar(paths, destination, true),
    }
}

fn create_zip(paths: &[PathBuf], destination: &Path) -> Result<(), String> {
    let file = File::create(destination).map_err(|error| error.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for path in paths {
        let root = path
            .file_name()
            .ok_or("Invalid path to compress")?
            .to_string_lossy()
            .into_owned();
        add_zip_entry(&mut zip, path, Path::new(&root), &options)?;
    }
    zip.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn add_zip_entry(
    zip: &mut ZipWriter<File>,
    source: &Path,
    rel: &Path,
    options: &SimpleFileOptions,
) -> Result<(), String> {
    if source.is_dir() {
        let dir = format!("{}/", rel.to_string_lossy().replace('\\', "/"));
        zip.add_directory(dir, *options)
            .map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let child = rel.join(entry.file_name());
            add_zip_entry(zip, &entry.path(), &child, options)?;
        }
    } else {
        let name = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(name, *options)
            .map_err(|error| error.to_string())?;
        let mut file = File::open(source).map_err(|error| error.to_string())?;
        io::copy(&mut file, zip).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn create_tar(paths: &[PathBuf], destination: &Path, gzip: bool) -> Result<(), String> {
    let file = File::create(destination).map_err(|error| error.to_string())?;
    let writer: Box<dyn Write> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    let mut builder = tar::Builder::new(writer);
    for path in paths {
        let root = path
            .file_name()
            .ok_or("Invalid path to compress")?
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            builder
                .append_dir_all(&root, path)
                .map_err(|error| error.to_string())?;
        } else {
            builder
                .append_path_with_name(path, &root)
                .map_err(|error| error.to_string())?;
        }
    }
    builder.finish().map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    /// Unsafe paths and links: listed for the count, never extracted.
    pub skipped: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveListing {
    pub entries: Vec<ArchiveEntry>,
    pub uncompressed_size: u64,
    pub truncated: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionTarget {
    pub name: String,
    pub is_directory: bool,
    pub exists: bool,
    pub existing_modified: Option<i64>,
}

/// One top-level item an extraction writes. An empty `root` wraps the whole archive in `name`.
#[derive(Clone, Debug, PartialEq)]
struct Target {
    name: String,
    is_directory: bool,
    root: String,
}

pub(crate) struct EntryInfo {
    pub path: String,
    pub is_directory: bool,
    pub size: u64,
    pub skipped: bool,
    mode: Option<u32>,
}

pub(crate) enum Flow {
    Continue,
    Stop,
}

/// Normalizes an entry name to `a/b/c` and reports whether it stays inside the destination.
fn normalize(name: &str) -> (String, bool) {
    let unified = name.replace('\\', "/");
    let path = unified.trim_end_matches('/').to_string();
    let safe = !path.is_empty()
        && !path.starts_with('/')
        && path.split('/').all(|segment| {
            !segment.is_empty() && segment != "." && segment != ".." && !segment.contains(':')
        });
    (path, safe)
}

/// Visits every entry in archive order. `visit` may read the entry body from the reader.
pub(crate) fn for_each_entry(
    archive: &Path,
    visit: &mut dyn FnMut(&EntryInfo, &mut dyn Read) -> Result<Flow, String>,
) -> Result<(), String> {
    let format = format_for(archive).ok_or(UNSUPPORTED)?;
    let file = BufReader::new(File::open(archive).map_err(to_string)?);
    match format {
        Format::Zip => for_each_zip_entry(file, visit),
        Format::Tar => for_each_tar_entry(file, visit),
        Format::TarGz => for_each_tar_entry(GzDecoder::new(file), visit),
    }
}

/// Visits every entry of a zip read from `reader`, in archive order.
pub(crate) fn for_each_zip_entry<R: Read + io::Seek>(
    reader: R,
    visit: &mut dyn FnMut(&EntryInfo, &mut dyn Read) -> Result<Flow, String>,
) -> Result<(), String> {
    let mut zip = ZipArchive::new(reader).map_err(to_string)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(to_string)?;
        let (path, safe) = normalize(entry.name());
        if path.is_empty() {
            continue;
        }
        let mode = entry.unix_mode();
        let link = mode.is_some_and(|mode| mode & 0o170000 == 0o120000);
        let info = EntryInfo {
            path,
            is_directory: entry.is_dir(),
            size: entry.size(),
            skipped: !safe || link,
            mode,
        };
        if let Flow::Stop = visit(&info, &mut entry)? {
            break;
        }
    }
    Ok(())
}

/// Visits every entry of an uncompressed tar stream, in archive order.
pub(crate) fn for_each_tar_entry<R: Read>(
    reader: R,
    visit: &mut dyn FnMut(&EntryInfo, &mut dyn Read) -> Result<Flow, String>,
) -> Result<(), String> {
    let mut tar = tar::Archive::new(reader);
    for entry in tar.entries().map_err(to_string)? {
        let mut entry = entry.map_err(to_string)?;
        let kind = entry.header().entry_type();
        let link = kind.is_symlink() || kind.is_hard_link();
        if !(kind.is_file() || kind.is_dir() || link) {
            continue;
        }
        let (path, safe) = normalize(&String::from_utf8_lossy(&entry.path_bytes()));
        if path.is_empty() {
            continue;
        }
        let info = EntryInfo {
            path,
            is_directory: kind.is_dir(),
            size: entry.size(),
            skipped: !safe || link,
            mode: entry.header().mode().ok(),
        };
        if let Flow::Stop = visit(&info, &mut entry)? {
            break;
        }
    }
    Ok(())
}

/// Adds folders that only appear as parents of other entries, so the tree is complete.
fn with_implied_directories(entries: Vec<ArchiveEntry>) -> Vec<ArchiveEntry> {
    let mut known: HashSet<String> = entries
        .iter()
        .filter(|entry| entry.is_directory && !entry.skipped)
        .map(|entry| entry.path.clone())
        .collect();
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        if !entry.skipped {
            let mut missing = Vec::new();
            let mut current = entry.path.as_str();
            while let Some(index) = current.rfind('/') {
                current = &current[..index];
                if known.insert(current.to_string()) {
                    missing.push(current.to_string());
                }
            }
            result.extend(missing.into_iter().rev().map(|path| ArchiveEntry {
                path,
                is_directory: true,
                size: 0,
                skipped: false,
            }));
        }
        result.push(entry);
    }
    result
}

pub fn read_listing(archive: &Path, limit: Option<usize>) -> Result<ArchiveListing, String> {
    let mut raw = Vec::new();
    let mut truncated = false;
    for_each_entry(archive, &mut |info, _| {
        if limit.is_some_and(|limit| raw.len() >= limit) {
            truncated = true;
            return Ok(Flow::Stop);
        }
        raw.push(ArchiveEntry {
            path: info.path.clone(),
            is_directory: info.is_directory,
            size: info.size,
            skipped: info.skipped,
        });
        Ok(Flow::Continue)
    })?;
    let entries = with_implied_directories(raw);
    let uncompressed_size = entries
        .iter()
        .filter(|entry| !entry.skipped && !entry.is_directory)
        .map(|entry| entry.size)
        .sum();
    Ok(ArchiveListing {
        entries,
        uncompressed_size,
        truncated,
    })
}

/// Archive file name without its archive extension, for the wrapping folder.
fn archive_stem(archive: &Path) -> String {
    let name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    let stem = [".tar.gz", ".tgz", ".tar", ".zip"]
        .iter()
        .find(|extension| lower.ends_with(*extension))
        .map(|extension| &name[..name.len() - extension.len()])
        .unwrap_or(&name);
    if stem.is_empty() {
        "Archive".to_string()
    } else {
        stem.to_string()
    }
}

fn plan_targets(
    entries: &[ArchiveEntry],
    archive: &Path,
    selection: Option<&[String]>,
) -> Result<Vec<Target>, String> {
    let safe: Vec<&ArchiveEntry> = entries.iter().filter(|entry| !entry.skipped).collect();
    let Some(selection) = selection else {
        let mut roots: Vec<&str> = safe
            .iter()
            .map(|entry| entry.path.split('/').next().unwrap_or_default())
            .collect();
        roots.sort_unstable();
        roots.dedup();
        return match roots.as_slice() {
            [] => Err("This archive has nothing to extract".into()),
            [root] => {
                let is_directory = safe
                    .iter()
                    .any(|entry| entry.path != *root || entry.is_directory);
                Ok(vec![Target {
                    name: root.to_string(),
                    is_directory,
                    root: root.to_string(),
                }])
            }
            _ => Ok(vec![Target {
                name: archive_stem(archive),
                is_directory: true,
                root: String::new(),
            }]),
        };
    };
    let mut chosen: Vec<&str> = selection
        .iter()
        .map(|path| path.trim_end_matches('/'))
        .collect();
    chosen.sort_unstable();
    chosen.dedup();
    let collapsed: Vec<&str> = chosen
        .iter()
        .copied()
        .filter(|path| {
            !chosen.iter().any(|other| {
                path.len() > other.len()
                    && path.starts_with(other)
                    && path.as_bytes()[other.len()] == b'/'
            })
        })
        .collect();
    let mut names = HashSet::new();
    collapsed
        .into_iter()
        .map(|path| {
            let entry = safe
                .iter()
                .find(|entry| entry.path == path)
                .ok_or_else(|| format!("“{path}” is not in the archive"))?;
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            if !names.insert(name.clone()) {
                return Err(format!("Two selected items are named “{name}”"));
            }
            Ok(Target {
                name,
                is_directory: entry.is_directory,
                root: path.to_string(),
            })
        })
        .collect()
}

fn modified_millis(metadata: &fs::Metadata) -> Option<i64> {
    let since_epoch = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some(since_epoch.as_millis() as i64)
}

pub fn plan(
    archive: &Path,
    destination: &Path,
    selection: Option<&[String]>,
) -> Result<Vec<ExtractionTarget>, String> {
    let listing = read_listing(archive, None)?;
    Ok(plan_targets(&listing.entries, archive, selection)?
        .into_iter()
        .map(|target| {
            let existing = fs::symlink_metadata(destination.join(&target.name)).ok();
            ExtractionTarget {
                exists: existing.is_some(),
                existing_modified: existing.as_ref().and_then(modified_millis),
                name: target.name,
                is_directory: target.is_directory,
            }
        })
        .collect())
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Resolution {
    Replace,
    KeepBoth,
    Skip,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TargetError {
    pub name: String,
    pub error: String,
}

#[derive(Serialize, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionOutcome {
    /// Final paths of the targets moved into the destination.
    pub extracted: Vec<String>,
    pub failed: Vec<TargetError>,
    pub cancelled: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
}

struct Staged {
    target: Target,
    staging: PathBuf,
    resolution: Option<Resolution>,
    error: Option<String>,
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Finder-style free name: `project 2`, `readme 2.md`.
fn unique_name(directory: &Path, name: &str, is_directory: bool) -> String {
    let (stem, extension) = match name.rfind('.') {
        Some(index) if index > 0 && !is_directory => name.split_at(index),
        _ => (name, ""),
    };
    (2..)
        .map(|number| format!("{stem} {number}{extension}"))
        .find(|candidate| !exists(&directory.join(candidate)))
        .expect("unbounded range")
}

/// Path of an archive entry inside a target, or `None` when the target does not cover it.
fn relative_in<'a>(root: &str, path: &'a str) -> Option<&'a str> {
    if root.is_empty() {
        return Some(path);
    }
    if path == root {
        return Some("");
    }
    path.strip_prefix(root)?.strip_prefix('/')
}

/// Streams one file. Returns `Ok(false)` when cancelled mid-copy.
fn write_file(
    out: &Path,
    reader: &mut dyn Read,
    mode: Option<u32>,
    cancel: &dyn Fn() -> bool,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<bool, String> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(to_string)?;
    }
    let mut file = File::create(out).map_err(to_string)?;
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        if cancel() {
            return Ok(false);
        }
        let read = reader.read(&mut buffer).map_err(to_string)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(to_string)?;
        on_bytes(read as u64);
    }
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(out, fs::Permissions::from_mode(mode & 0o777)).map_err(to_string)?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(true)
}

/// Moves a fully staged target into place, applying its conflict resolution.
fn finalize(item: &Staged, destination: &Path) -> Result<PathBuf, String> {
    if item.target.is_directory && !exists(&item.staging) {
        fs::create_dir_all(&item.staging).map_err(to_string)?;
    }
    let mut path = destination.join(&item.target.name);
    if exists(&path) {
        match item.resolution {
            Some(Resolution::Replace) => remove_path(&path).map_err(to_string)?,
            Some(Resolution::KeepBoth) => {
                path = destination.join(unique_name(
                    destination,
                    &item.target.name,
                    item.target.is_directory,
                ))
            }
            _ => return Err(format!("“{}” already exists", item.target.name)),
        }
    }
    fs::rename(&item.staging, &path).map_err(to_string)?;
    Ok(path)
}

pub fn extract_targets(
    archive: &Path,
    destination: &Path,
    selection: Option<&[String]>,
    resolutions: &HashMap<String, Resolution>,
    cancel: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(&Counts),
) -> Result<ExtractionOutcome, String> {
    if !exists(archive) {
        return Err(format!("{} does not exist", archive.display()));
    }
    fs::create_dir_all(destination).map_err(to_string)?;
    let listing = read_listing(archive, None)?;
    let mut outcome = ExtractionOutcome::default();
    let mut staged = Vec::new();
    for target in plan_targets(&listing.entries, archive, selection)? {
        let resolution = resolutions.get(&target.name).copied();
        if resolution == Some(Resolution::Skip) {
            continue;
        }
        if resolution.is_none() && exists(&destination.join(&target.name)) {
            outcome.failed.push(TargetError {
                error: format!("“{}” already exists", target.name),
                name: target.name,
            });
            continue;
        }
        let staging =
            destination.join(format!(".{}.partial-{}", target.name, uuid::Uuid::new_v4()));
        staged.push(Staged {
            target,
            staging,
            resolution,
            error: None,
        });
    }

    let mut counts = Counts::default();
    for entry in listing
        .entries
        .iter()
        .filter(|entry| !entry.skipped && !entry.is_directory)
    {
        if staged
            .iter()
            .any(|item| relative_in(&item.target.root, &entry.path).is_some())
        {
            counts.files_total += 1;
            counts.bytes_total += entry.size;
        }
    }
    on_progress(&counts);

    let mut cancelled = false;
    let walk = if staged.is_empty() {
        Ok(())
    } else {
        for_each_entry(archive, &mut |info, reader| {
            if info.skipped {
                return Ok(Flow::Continue);
            }
            if cancel() {
                cancelled = true;
                return Ok(Flow::Stop);
            }
            let Some(item) = staged.iter_mut().find(|item| {
                item.error.is_none() && relative_in(&item.target.root, &info.path).is_some()
            }) else {
                return Ok(Flow::Continue);
            };
            let relative = relative_in(&item.target.root, &info.path).unwrap_or_default();
            let out = if relative.is_empty() {
                item.staging.clone()
            } else {
                item.staging.join(relative)
            };
            if info.is_directory {
                if let Err(error) = fs::create_dir_all(&out) {
                    item.error = Some(error.to_string());
                }
                return Ok(Flow::Continue);
            }
            let written = write_file(&out, reader, info.mode, cancel, &mut |bytes| {
                counts.bytes_done += bytes;
                on_progress(&counts);
            });
            match written {
                Ok(true) => {
                    counts.files_done += 1;
                    on_progress(&counts);
                }
                Ok(false) => {
                    cancelled = true;
                    return Ok(Flow::Stop);
                }
                Err(error) => item.error = Some(error),
            }
            Ok(Flow::Continue)
        })
    };

    if let Err(error) = walk {
        for item in &staged {
            let _ = remove_path(&item.staging);
        }
        return Err(error);
    }
    if cancelled {
        for item in &staged {
            let _ = remove_path(&item.staging);
        }
        outcome.cancelled = true;
        return Ok(outcome);
    }
    for item in staged {
        let result = match &item.error {
            Some(error) => Err(error.clone()),
            None => finalize(&item, destination),
        };
        match result {
            Ok(path) => outcome.extracted.push(path.to_string_lossy().into_owned()),
            Err(error) => {
                let _ = remove_path(&item.staging);
                outcome.failed.push(TargetError {
                    name: item.target.name,
                    error,
                });
            }
        }
    }
    Ok(outcome)
}

#[tauri::command]
pub async fn create_archive(paths: Vec<String>, destination: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        create(&paths, &PathBuf::from(destination))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn list_archive(path: String) -> Result<ArchiveListing, String> {
    tauri::async_runtime::spawn_blocking(move || read_listing(Path::new(&path), Some(LIST_LIMIT)))
        .await
        .map_err(to_string)?
}

#[tauri::command]
pub async fn plan_extraction(
    archive: String,
    destination: String,
    entries: Option<Vec<String>>,
) -> Result<Vec<ExtractionTarget>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        plan(
            Path::new(&archive),
            Path::new(&destination),
            entries.as_deref(),
        )
    })
    .await
    .map_err(to_string)?
}

/// Extracts under a job id chosen by the frontend and resolves when the job ends.
/// Progress goes out as `transfer-progress` events; `cancel_transfer` stops it.
#[tauri::command]
pub async fn extract_archive(
    app: tauri::AppHandle,
    registry: tauri::State<'_, crate::transfer::TransferRegistry>,
    job_id: String,
    archive: String,
    destination: String,
    entries: Option<Vec<String>>,
    resolutions: HashMap<String, Resolution>,
) -> Result<ExtractionOutcome, String> {
    let token = registry.register(&job_id);
    let id = job_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let archive_path = PathBuf::from(&archive);
        let mut event =
            crate::transfer::TransferEvent::new(id, "extract", destination.clone(), 0, 0);
        event.label = archive_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut last_emit: Option<Instant> = None;
        let cancel = || token.is_cancelled();
        let outcome = extract_targets(
            &archive_path,
            Path::new(&destination),
            entries.as_deref(),
            &resolutions,
            &cancel,
            &mut |counts| {
                if last_emit.is_some_and(|at| at.elapsed() < Duration::from_millis(100)) {
                    return;
                }
                last_emit = Some(Instant::now());
                event.files_total = counts.files_total;
                event.files_done = counts.files_done;
                event.bytes_total = counts.bytes_total;
                event.bytes_done = counts.bytes_done;
                crate::transfer::emit(&app, &event);
            },
        );
        event.state = match &outcome {
            Ok(done) if done.cancelled => "cancelled",
            Ok(done) if done.failed.is_empty() => "done",
            _ => "failed",
        }
        .to_string();
        event.error = match &outcome {
            Ok(done) => done
                .failed
                .first()
                .map(|failure| format!("{}: {}", failure.name, failure.error)),
            Err(error) => Some(error.clone()),
        };
        if let Ok(done) = &outcome {
            if !done.cancelled && done.failed.is_empty() {
                event.files_done = event.files_total;
                event.bytes_done = event.bytes_total;
            }
        }
        crate::transfer::emit(&app, &event);
        outcome
    })
    .await
    .map_err(to_string);
    registry.remove(&job_id);
    result?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lite-explorer-archive-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_with(dir: &Path, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default();
        for (entry, bytes) in files {
            if entry.ends_with('/') {
                zip.add_directory(*entry, options).unwrap();
            } else {
                zip.start_file(*entry, options).unwrap();
                zip.write_all(bytes).unwrap();
            }
        }
        zip.finish().unwrap();
        path
    }

    fn tar_gz_with(
        dir: &Path,
        name: &str,
        files: &[(&str, &[u8])],
        symlink: Option<(&str, &str)>,
    ) -> PathBuf {
        let path = dir.join(name);
        let encoder = GzEncoder::new(File::create(&path).unwrap(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (entry, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            builder.append_data(&mut header, entry, *bytes).unwrap();
        }
        if let Some((link, target)) = symlink {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            builder.append_link(&mut header, link, target).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn summary(listing: &ArchiveListing) -> Vec<(&str, bool, bool)> {
        listing
            .entries
            .iter()
            .map(|entry| (entry.path.as_str(), entry.is_directory, entry.skipped))
            .collect()
    }

    fn entries(paths: &[(&str, bool)]) -> Vec<ArchiveEntry> {
        paths
            .iter()
            .map(|(path, is_directory)| ArchiveEntry {
                path: path.to_string(),
                is_directory: *is_directory,
                size: 0,
                skipped: false,
            })
            .collect()
    }

    fn no_cancel() -> bool {
        false
    }

    fn extract_all(
        archive: &Path,
        destination: &Path,
        resolutions: &[(&str, Resolution)],
    ) -> ExtractionOutcome {
        let resolutions = resolutions
            .iter()
            .map(|(name, resolution)| (name.to_string(), *resolution))
            .collect();
        extract_targets(
            archive,
            destination,
            None,
            &resolutions,
            &no_cancel,
            &mut |_| {},
        )
        .unwrap()
    }

    #[test]
    fn detects_formats() {
        assert_eq!(format_for(Path::new("a.zip")), Some(Format::Zip));
        assert_eq!(format_for(Path::new("a.TAR.GZ")), Some(Format::TarGz));
        assert_eq!(format_for(Path::new("a.tgz")), Some(Format::TarGz));
        assert_eq!(format_for(Path::new("a.tar")), Some(Format::Tar));
        assert_eq!(format_for(Path::new("a.txt")), None);
    }

    #[test]
    fn zip_round_trip() {
        let dir = temp_dir("zip");
        let source = dir.join("hello.txt");
        fs::write(&source, b"hi there").unwrap();
        let archive = dir.join("out.zip");
        create(&[source.clone()], &archive).unwrap();
        let out = dir.join("out");
        let outcome = extract_all(&archive, &out, &[]);
        assert_eq!(fs::read(out.join("hello.txt")).unwrap(), b"hi there");
        assert_eq!(
            outcome.extracted,
            vec![out.join("hello.txt").to_string_lossy().into_owned()]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tar_gz_round_trip_preserves_folders() {
        let dir = temp_dir("targz");
        let folder = dir.join("docs");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("note.md"), b"# note").unwrap();
        let archive = dir.join("out.tar.gz");
        create(&[folder], &archive).unwrap();
        let out = dir.join("out");
        extract_all(&archive, &out, &[]);
        assert_eq!(fs::read(out.join("docs/note.md")).unwrap(), b"# note");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_unknown_format() {
        let dir = temp_dir("bad");
        let source = dir.join("x.txt");
        fs::write(&source, b"x").unwrap();
        assert!(create(&[source], &dir.join("out.rar")).is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lists_zip_with_implied_directories_and_skipped_entries() {
        let dir = temp_dir("list-zip");
        let archive = zip_with(
            &dir,
            "a.zip",
            &[
                ("src/lib/util.ts", b"x"),
                ("README.md", b"hello"),
                ("../escape.txt", b"no"),
            ],
        );
        let listing = read_listing(&archive, None).unwrap();
        assert_eq!(
            summary(&listing),
            vec![
                ("src", true, false),
                ("src/lib", true, false),
                ("src/lib/util.ts", false, false),
                ("README.md", false, false),
                ("../escape.txt", false, true),
            ]
        );
        assert_eq!(listing.uncompressed_size, 6);
        assert!(!listing.truncated);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lists_tar_gz_and_skips_links() {
        let dir = temp_dir("list-tar");
        let archive = tar_gz_with(
            &dir,
            "a.tar.gz",
            &[("docs/note.md", b"# note")],
            Some(("docs/link", "/etc/passwd")),
        );
        let listing = read_listing(&archive, None).unwrap();
        assert_eq!(
            summary(&listing),
            vec![
                ("docs", true, false),
                ("docs/note.md", false, false),
                ("docs/link", false, true)
            ]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_stops_at_the_limit() {
        let dir = temp_dir("list-limit");
        let archive = zip_with(
            &dir,
            "a.zip",
            &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")],
        );
        let listing = read_listing(&archive, Some(2)).unwrap();
        assert_eq!(listing.entries.len(), 2);
        assert!(listing.truncated);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn smart_plan_uses_single_root_or_wraps_in_archive_named_folder() {
        let archive = Path::new("/tmp/backup.tar.gz");
        assert_eq!(
            plan_targets(
                &entries(&[("project", true), ("project/a.txt", false)]),
                archive,
                None
            )
            .unwrap(),
            vec![Target {
                name: "project".into(),
                is_directory: true,
                root: "project".into()
            }]
        );
        assert_eq!(
            plan_targets(&entries(&[("notes.txt", false)]), archive, None).unwrap(),
            vec![Target {
                name: "notes.txt".into(),
                is_directory: false,
                root: "notes.txt".into()
            }]
        );
        assert_eq!(
            plan_targets(
                &entries(&[("a.txt", false), ("b.txt", false)]),
                archive,
                None
            )
            .unwrap(),
            vec![Target {
                name: "backup".into(),
                is_directory: true,
                root: String::new()
            }]
        );
        assert!(plan_targets(&[], archive, None).is_err());
    }

    #[test]
    fn selection_plan_collapses_to_ancestors_and_uses_base_names() {
        let archive = Path::new("/tmp/a.zip");
        let listed = entries(&[
            ("src", true),
            ("src/lib", true),
            ("src/lib/a.ts", false),
            ("docs", true),
            ("docs/lib", true),
        ]);
        let selection = vec!["src/lib/".to_string(), "src/lib/a.ts".to_string()];
        assert_eq!(
            plan_targets(&listed, archive, Some(selection.as_slice())).unwrap(),
            vec![Target {
                name: "lib".into(),
                is_directory: true,
                root: "src/lib".into()
            }]
        );
        let duplicate = vec!["src/lib".to_string(), "docs/lib".to_string()];
        let error = plan_targets(&listed, archive, Some(duplicate.as_slice())).unwrap_err();
        assert!(error.contains("lib"));
    }

    #[test]
    fn plan_reports_existing_targets() {
        let dir = temp_dir("plan");
        let archive = zip_with(&dir, "a.zip", &[("project/", b""), ("project/a.txt", b"x")]);
        let destination = dir.join("out");
        fs::create_dir_all(destination.join("project")).unwrap();
        let targets = plan(&archive, &destination, None).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "project");
        assert!(targets[0].is_directory);
        assert!(targets[0].exists);
        assert!(targets[0].existing_modified.is_some());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn wraps_multiple_top_level_entries_in_archive_named_folder() {
        let dir = temp_dir("wrap");
        let archive = zip_with(&dir, "bundle.zip", &[("a.txt", b"a"), ("b/c.txt", b"c")]);
        let out = dir.join("out");
        extract_all(&archive, &out, &[]);
        assert_eq!(fs::read(out.join("bundle/a.txt")).unwrap(), b"a");
        assert_eq!(fs::read(out.join("bundle/b/c.txt")).unwrap(), b"c");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn extracts_selection_under_base_names() {
        let dir = temp_dir("selection");
        let archive = zip_with(
            &dir,
            "a.zip",
            &[("src/lib/a.ts", b"a"), ("src/main.ts", b"m")],
        );
        let out = dir.join("out");
        let selection = vec!["src/lib/".to_string()];
        extract_targets(
            &archive,
            &out,
            Some(selection.as_slice()),
            &HashMap::new(),
            &no_cancel,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(fs::read(out.join("lib/a.ts")).unwrap(), b"a");
        assert!(!out.join("src").exists());
        assert!(!out.join("main.ts").exists());
        fs::remove_dir_all(&dir).ok();
    }

    fn project_with_existing(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = temp_dir(tag);
        let archive = zip_with(&dir, "a.zip", &[("project/new.txt", b"new")]);
        let out = dir.join("out");
        fs::create_dir_all(out.join("project")).unwrap();
        fs::write(out.join("project/old.txt"), b"old").unwrap();
        (dir, archive, out)
    }

    #[test]
    fn missing_resolution_keeps_the_existing_target() {
        let (dir, archive, out) = project_with_existing("missing");
        let outcome = extract_all(&archive, &out, &[]);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].name, "project");
        assert!(out.join("project/old.txt").exists());
        assert!(!out.join("project/new.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn keep_both_uses_finder_style_names() {
        let (dir, archive, out) = project_with_existing("keep-both");
        let outcome = extract_all(&archive, &out, &[("project", Resolution::KeepBoth)]);
        assert_eq!(fs::read(out.join("project 2/new.txt")).unwrap(), b"new");
        assert!(out.join("project/old.txt").exists());
        assert_eq!(
            outcome.extracted,
            vec![out.join("project 2").to_string_lossy().into_owned()]
        );

        let file_archive = zip_with(&dir, "notes.zip", &[("notes.txt", b"n")]);
        fs::write(out.join("notes.txt"), b"old").unwrap();
        extract_all(&file_archive, &out, &[("notes.txt", Resolution::KeepBoth)]);
        assert_eq!(fs::read(out.join("notes 2.txt")).unwrap(), b"n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replace_swaps_the_folder_after_staging() {
        let (dir, archive, out) = project_with_existing("replace");
        extract_all(&archive, &out, &[("project", Resolution::Replace)]);
        assert_eq!(fs::read(out.join("project/new.txt")).unwrap(), b"new");
        assert!(!out.join("project/old.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skip_leaves_everything_in_place() {
        let (dir, archive, out) = project_with_existing("skip");
        let outcome = extract_all(&archive, &out, &[("project", Resolution::Skip)]);
        assert!(outcome.extracted.is_empty());
        assert!(outcome.failed.is_empty());
        assert!(out.join("project/old.txt").exists());
        assert!(!out.join("project/new.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancellation_leaves_nothing_in_the_destination() {
        let dir = temp_dir("cancel");
        let archive = zip_with(
            &dir,
            "a.zip",
            &[("a.txt", b"a"), ("b.txt", b"b"), ("c.txt", b"c")],
        );
        let out = dir.join("out");
        let calls = Cell::new(0);
        let cancel = || {
            calls.set(calls.get() + 1);
            calls.get() > 2
        };
        let outcome =
            extract_targets(&archive, &out, None, &HashMap::new(), &cancel, &mut |_| {}).unwrap();
        assert!(outcome.cancelled);
        assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsafe_entries_never_escape_the_destination() {
        let dir = temp_dir("escape");
        let inner = dir.join("inner");
        fs::create_dir_all(&inner).unwrap();
        let archive = zip_with(
            &inner,
            "a.zip",
            &[("../escape.txt", b"no"), ("ok.txt", b"ok")],
        );
        let out = inner.join("out");
        extract_all(&archive, &out, &[]);
        assert_eq!(fs::read(out.join("ok.txt")).unwrap(), b"ok");
        assert!(!inner.join("escape.txt").exists());
        assert!(!dir.join("escape.txt").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn progress_counts_files_and_bytes() {
        let dir = temp_dir("progress");
        let archive = zip_with(&dir, "a.zip", &[("a.txt", b"abc"), ("b.txt", b"de")]);
        let mut last = Counts::default();
        extract_targets(
            &archive,
            &dir.join("out"),
            None,
            &HashMap::new(),
            &no_cancel,
            &mut |counts| last = *counts,
        )
        .unwrap();
        assert_eq!(
            last,
            Counts {
                files_total: 2,
                files_done: 2,
                bytes_total: 5,
                bytes_done: 5
            }
        );
        fs::remove_dir_all(&dir).ok();
    }
}
