use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use ignore::WalkState;

use crate::{epoch_millis, explorer::archive::Flow, DirectoryEntry};
use serde::{Deserialize, Serialize};

use super::containers::{ArchiveKind, Codec};
use super::documents::DocumentKind;
use super::mail::MailKind;
#[cfg(target_os = "macos")]
use super::spotlight;
use super::{containers, documents, mail, text};

const MAX_RESULTS: usize = 1_000;
const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024;
/// RTF often embeds images as hex, so it gets more room than plain text.
const MAX_RTF_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MESSAGE_BYTES: u64 = 64 * 1024 * 1024;
/// Archives, compressed files and mailboxes are streamed; larger ones are skipped to keep a
/// search responsive.
const MAX_CONTAINER_BYTES: u64 = 256 * 1024 * 1024;
/// Longest snippet returned for a match, in characters.
const SNIPPET_CHARS: usize = 220;
/// Results found so far are sent at least this often, so the list fills while the search runs.
const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// In-memory budget for extracted text kept between searches, so unchanged files are not re-read.
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchMode {
    Fuzzy,
    Content,
}

#[derive(Serialize)]
pub(crate) struct SearchResult {
    #[serde(flatten)]
    pub entry: DirectoryEntry,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    /// Path of the matching file inside an archive result, e.g. `docs/readme.txt`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_path: Option<String>,
}

#[cfg(test)]
pub(crate) struct SearchResponse {
    pub results: Vec<SearchResult>,
}

/// Totals reported once a streamed search ends.
#[derive(Default)]
pub(crate) struct SearchSummary {
    /// Files content search could not read: too large, binary or unreadable.
    pub skipped: usize,
    /// Whether the search stopped at [`MAX_RESULTS`].
    pub limited: bool,
}

/// Extracted text kept in memory so repeated content searches skip unchanged files.
#[derive(Clone, Default)]
pub(crate) struct ContentCache {
    inner: Arc<Mutex<Cache>>,
}

#[derive(Default)]
struct Cache {
    map: HashMap<PathBuf, CachedText>,
    bytes: usize,
}

struct CachedText {
    modified: SystemTime,
    size: u64,
    /// `None` marks a file already found unreadable/binary.
    text: Option<String>,
}

impl ContentCache {
    fn get(&self, path: &Path, modified: SystemTime, size: u64) -> Option<Option<String>> {
        self.inner
            .lock()
            .unwrap()
            .map
            .get(path)
            .filter(|cached| cached.modified == modified && cached.size == size)
            .map(|cached| cached.text.clone())
    }

    fn insert(&self, path: PathBuf, modified: SystemTime, size: u64, text: Option<String>) {
        let mut cache = self.inner.lock().unwrap();
        let cost = text.as_ref().map_or(0, String::len);
        if cache.bytes + cost > MAX_CACHE_BYTES {
            cache.map.clear();
            cache.bytes = 0;
        }
        cache.bytes += cost;
        cache.map.insert(path, CachedText { modified, size, text });
    }
}

fn matches(haystack: &str, query: &str) -> bool {
    if query.contains(['*', '?']) {
        return glob_matches(&haystack.to_lowercase(), &query.to_lowercase());
    }
    let lower = haystack.to_lowercase();
    let mut chars = lower.chars();
    query
        .to_lowercase()
        .chars()
        .all(|needle| chars.any(|c| c == needle))
}

fn glob_matches(text: &str, pattern: &str) -> bool {
    let (text, pattern) = (text.as_bytes(), pattern.as_bytes());
    let (mut ti, mut pi, mut star, mut retry) = (0, 0, None, 0);
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) {
            ti += 1;
            pi += 1;
        } else if pi < pattern.len() && pattern[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            retry = ti;
        } else if let Some(at) = star {
            pi = at + 1;
            retry += 1;
            ti = retry;
        } else {
            return false;
        }
    }
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }
    pi == pattern.len()
}

fn entry(path: &Path) -> Option<DirectoryEntry> {
    let metadata = fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(DirectoryEntry {
        name: name.clone(),
        path: path.to_string_lossy().into_owned(),
        is_directory: metadata.is_dir(),
        is_hidden: name.starts_with('.'),
        size: (!metadata.is_dir()).then_some(metadata.len()),
        created: epoch_millis(metadata.created()),
        modified: epoch_millis(metadata.modified()),
        kind: None,
    })
}

/// Walks `root` on several threads and hands every path `accept` takes to `found`, skipping
/// symlinks. Directories are passed only when asked: content search reads files, name search
/// also matches folders. Stops early once `halt` is set or `found` refuses a path.
fn walk(
    root: &Path,
    include_directories: bool,
    halt: &AtomicBool,
    found: impl Fn(PathBuf) -> bool + Sync,
) {
    let found = &found;
    ignore::WalkBuilder::new(root)
        // No .gitignore or hidden-file rules: search sees what the listing shows.
        .standard_filters(false)
        .follow_links(false)
        .threads(worker_count())
        .build_parallel()
        .run(|| {
            Box::new(move |entry| {
                if halt.load(Ordering::Relaxed) {
                    return WalkState::Quit;
                }
                // Protected or unreadable folders (for example TCC-guarded paths without
                // Full Disk Access) should be skipped, not abort the whole search.
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                let Some(kind) = entry.file_type() else {
                    return WalkState::Continue;
                };
                if entry.depth() == 0 || kind.is_symlink() || kind.is_dir() && !include_directories {
                    return WalkState::Continue;
                }
                if found(entry.into_path()) {
                    WalkState::Continue
                } else {
                    WalkState::Quit
                }
            })
        });
}

fn worker_count() -> usize {
    thread::available_parallelism().map_or(4, |count| count.get()).clamp(2, 8)
}

/// A content match: matching line, plus the entry path when the match is inside an archive.
type ContentHit = (PathBuf, String, Option<String>);

/// How content search reads a file, decided by its name.
#[derive(Debug, PartialEq)]
enum FileKind<'a> {
    Archive(ArchiveKind),
    /// A single compressed file and the name of the file inside it.
    Compressed(Codec, &'a str),
    Document(DocumentKind),
    Mail(MailKind),
    Rtf,
    Text,
}

fn classify(name: &str) -> FileKind<'_> {
    let lower = name.to_lowercase();
    if let Some(kind) = containers::archive_kind(&lower) {
        return FileKind::Archive(kind);
    }
    if let Some((codec, inner)) = containers::compressed(name) {
        return FileKind::Compressed(codec, inner);
    }
    let extension = lower
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension);
    if let Some(kind) = documents::kind_for(extension) {
        return FileKind::Document(kind);
    }
    if let Some(kind) = mail::kind_for(extension) {
        return FileKind::Mail(kind);
    }
    if extension == "rtf" {
        FileKind::Rtf
    } else {
        FileKind::Text
    }
}

/// Largest file of this kind read into memory. Containers are never nested, so their limit is 0.
fn memory_limit(kind: &FileKind) -> u64 {
    match kind {
        FileKind::Archive(_) | FileKind::Compressed(..) => 0,
        FileKind::Document(kind) => documents::max_bytes(*kind),
        FileKind::Mail(_) => MAX_MESSAGE_BYTES,
        FileKind::Rtf => MAX_RTF_BYTES,
        FileKind::Text => MAX_CONTENT_BYTES,
    }
}

/// Extracts all searchable text from a single file, independent of the query.
/// `Err` marks a skipped file (too large, binary or unreadable).
fn extract_text(kind: &FileKind, bytes: &[u8]) -> Result<String, ()> {
    match kind {
        FileKind::Text => text::decode(bytes).map(|cow| cow.into_owned()).ok_or(()),
        FileKind::Rtf => Ok(text::rtf_text(bytes)),
        FileKind::Document(kind) => documents::document_text(bytes, *kind),
        FileKind::Mail(kind) => mail::mail_text(bytes, *kind).ok_or(()),
        FileKind::Archive(_) | FileKind::Compressed(..) => Err(()),
    }
}

/// Searches one file for `needle` (already lowercased). `Err` marks a skipped file: too large,
/// binary or unreadable.
fn content_match(
    path: PathBuf,
    needle: &str,
    cache: &ContentCache,
) -> Result<Option<ContentHit>, ()> {
    let meta = fs::metadata(&path).map_err(|_| ())?;
    let len = meta.len();
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = classify(&name);
    let open = || fs::File::open(&path).map(BufReader::new).map_err(|_| ());
    let line = match kind {
        FileKind::Archive(kind) => {
            if len > MAX_CONTAINER_BYTES {
                return Err(());
            }
            return Ok(
                archive_match(&path, kind, needle)?.map(|(inner, line)| (path, line, Some(inner)))
            );
        }
        FileKind::Compressed(codec, inner) => {
            let inner_kind = classify(inner);
            if len > MAX_CONTAINER_BYTES
                || matches!(inner_kind, FileKind::Archive(_) | FileKind::Compressed(..))
            {
                return Err(());
            }
            let bytes = read_limited(
                containers::decoder(codec, open()?)?,
                memory_limit(&inner_kind),
            )?;
            bytes_match(&inner_kind, &bytes, needle)?
        }
        FileKind::Mail(MailKind::Mbox) => {
            if len > MAX_CONTAINER_BYTES {
                return Err(());
            }
            mail::mbox_match(open()?, needle)
        }
        _ => {
            let limit = memory_limit(&kind);
            if len > limit {
                return Err(());
            }
            if let Some(cached) = cache.get(&path, modified, len) {
                return match cached {
                    Some(text) => Ok(matching_line(&text, needle).map(|line| (path, line, None))),
                    None => Err(()),
                };
            }
            let text = match extract_text(&kind, &read_limited(open()?, limit)?) {
                Ok(text) => text,
                Err(()) => {
                    cache.insert(path, modified, len, None);
                    return Err(());
                }
            };
            cache.insert(path.clone(), modified, len, Some(text.clone()));
            matching_line(&text, needle)
        }
    };
    Ok(line.map(|line| (path, line, None)))
}

/// Reads at most `limit` bytes; `Err` when there is more.
fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, ()> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > limit {
        Err(())
    } else {
        Ok(bytes)
    }
}

/// Matches contents already in memory. `Err` for binary or unreadable content.
fn bytes_match(kind: &FileKind, bytes: &[u8], needle: &str) -> Result<Option<String>, ()> {
    match kind {
        FileKind::Archive(_) | FileKind::Compressed(..) => Ok(None),
        FileKind::Document(kind) => documents::document_match(bytes, *kind, needle),
        FileKind::Mail(kind) => Ok(mail::mail_match(bytes, *kind, needle)),
        FileKind::Rtf => Ok(matching_line(&text::rtf_text(bytes), needle)),
        FileKind::Text => Ok(matching_line(&text::decode(bytes).ok_or(())?, needle)),
    }
}

/// First archive entry whose contents match, as (entry path, line). Nested archives and
/// oversized or binary entries are passed over.
fn archive_match(
    path: &Path,
    kind: ArchiveKind,
    needle: &str,
) -> Result<Option<(String, String)>, ()> {
    let mut found = None;
    containers::for_each_entry(path, kind, &mut |info, reader| {
        let kind = classify(&info.path);
        let limit = memory_limit(&kind);
        if info.is_directory || info.skipped || info.size > limit {
            return Ok(Flow::Continue);
        }
        let Ok(bytes) = read_limited(reader, limit) else {
            return Ok(Flow::Continue);
        };
        if let Ok(Some(line)) = bytes_match(&kind, &bytes, needle) {
            found = Some((info.path.clone(), line));
            return Ok(Flow::Stop);
        }
        Ok(Flow::Continue)
    })?;
    Ok(found)
}

/// First line containing `needle` (already lowercased), cut to a snippet around the match.
/// Lowercases line by line: lowercasing can change byte lengths (e.g. `İ`), so an offset
/// found in a lowercased copy of the whole text is not a valid offset into the original.
pub(crate) fn matching_line(text: &str, needle: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if !line.to_lowercase().contains(needle) {
            return None;
        }
        Some(snippet(line, match_char_index(line, needle).unwrap_or(0)))
    })
}

/// Character index in `line` where `needle` matches, lowercasing one character at a time so
/// positions map back to the original.
fn match_char_index(line: &str, needle: &str) -> Option<usize> {
    let mut lower = String::with_capacity(line.len());
    let mut origin = Vec::with_capacity(line.len());
    for (index, c) in line.chars().enumerate() {
        lower.extend(c.to_lowercase());
        origin.resize(lower.len(), index);
    }
    lower.find(needle).map(|at| origin[at])
}

/// Up to [`SNIPPET_CHARS`] characters of `line`, starting a little before `at` when the line is long.
fn snippet(line: &str, at: usize) -> String {
    let count = line.chars().count();
    if count <= SNIPPET_CHARS {
        return line.to_string();
    }
    let start = at.saturating_sub(60).min(count - SNIPPET_CHARS);
    let mut out: String = line.chars().skip(start).take(SNIPPET_CHARS).collect();
    if start > 0 {
        out.insert(0, '…');
    }
    if start + SNIPPET_CHARS < count {
        out.push('…');
    }
    out
}

/// What a search producer found: a name match, a content match, or a file it had to skip.
enum Hit {
    Name(PathBuf),
    Content(ContentHit),
    Skipped,
}

/// Searches `root` and hands results to `emit` in batches while it runs, so the first matches
/// show before the whole tree is read. Returns early once `cancelled` is set.
pub(crate) fn search_streaming(
    root: &Path,
    query: &str,
    mode: SearchMode,
    cache: &ContentCache,
    cancelled: &AtomicBool,
    emit: &mut dyn FnMut(Vec<SearchResult>),
) -> Result<SearchSummary, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    // Producers stop on this: set when the caller cancels or the result limit is reached.
    let halt = AtomicBool::new(false);
    let halt = &halt;
    let (hits_tx, hits_rx) = mpsc::channel::<Hit>();
    let mut summary = SearchSummary::default();
    thread::scope(|scope| {
        match mode {
            SearchMode::Fuzzy => {
                #[cfg(target_os = "macos")]
                if spotlight::can_search(query) {
                    let tx = hits_tx.clone();
                    scope.spawn(move || {
                        // Spotlight answers from its index, so matches show before the walk
                        // gets far. The walk still runs for hidden and unindexed files.
                        spotlight::find_names(root, query, halt, |path| {
                            let name = path.file_name().unwrap_or_default().to_string_lossy();
                            !matches(&name, query) || tx.send(Hit::Name(path.clone())).is_ok()
                        });
                    });
                }
                let tx = hits_tx.clone();
                scope.spawn(move || {
                    walk(root, true, halt, |path| {
                        // Match the name only, so a folder match does not pull in everything inside it.
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        !matches(&name, query) || tx.send(Hit::Name(path.clone())).is_ok()
                    });
                });
            }
            SearchMode::Content => {
                let (jobs_tx, jobs_rx) = mpsc::channel::<PathBuf>();
                let jobs_rx = Arc::new(Mutex::new(jobs_rx));
                let needle = query.to_lowercase();
                for _ in 0..worker_count() {
                    let (rx, tx, needle) = (jobs_rx.clone(), hits_tx.clone(), needle.clone());
                    scope.spawn(move || loop {
                        let Ok(path) = rx.lock().unwrap().recv() else {
                            break;
                        };
                        if halt.load(Ordering::Relaxed) {
                            break;
                        }
                        let hit = match content_match(path, &needle, cache) {
                            Ok(Some(hit)) => Hit::Content(hit),
                            Ok(None) => continue,
                            Err(()) => Hit::Skipped,
                        };
                        if tx.send(hit).is_err() {
                            break;
                        }
                    });
                }
                scope.spawn(move || walk(root, false, halt, |path| jobs_tx.send(path).is_ok()));
            }
        }
        drop(hits_tx);
        collect_hits(root, &hits_rx, cancelled, &mut summary, emit);
        halt.store(true, Ordering::Relaxed);
        // Producers blocked on a full search notice the closed channel and stop.
        drop(hits_rx);
    });
    Ok(summary)
}

/// Turns hits into results and emits them in batches, until producers finish, the caller
/// cancels, or [`MAX_RESULTS`] is reached.
fn collect_hits(
    root: &Path,
    hits: &mpsc::Receiver<Hit>,
    cancelled: &AtomicBool,
    summary: &mut SearchSummary,
    emit: &mut dyn FnMut(Vec<SearchResult>),
) {
    // Spotlight and the walk report the same files; each is listed once.
    let mut seen = HashSet::new();
    let mut batch = Vec::new();
    let mut count = 0;
    let mut flushed_at = Instant::now();
    loop {
        match hits.recv_timeout(FLUSH_EVERY) {
            Ok(Hit::Skipped) => summary.skipped += 1,
            Ok(Hit::Name(path)) => {
                if let Some(result) = search_result(root, &mut seen, path, None, None) {
                    batch.push(result);
                    count += 1;
                }
            }
            Ok(Hit::Content((path, snippet, inner_path))) => {
                if let Some(result) = search_result(root, &mut seen, path, Some(snippet), inner_path) {
                    batch.push(result);
                    count += 1;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if count >= MAX_RESULTS {
            summary.limited = true;
            break;
        }
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        if !batch.is_empty() && flushed_at.elapsed() >= FLUSH_EVERY {
            flushed_at = Instant::now();
            emit(std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        emit(batch);
    }
}

fn search_result(
    root: &Path,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    snippet: Option<String>,
    inner_path: Option<String>,
) -> Option<SearchResult> {
    // Spotlight may answer for paths outside the folder, e.g. through a symlinked root.
    if !path.starts_with(root) || !seen.insert(path.clone()) {
        return None;
    }
    let entry = entry(&path)?;
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    Some(SearchResult {
        entry,
        relative_path,
        snippet,
        inner_path,
    })
}

/// The whole search at once, for callers that don't stream.
#[cfg(test)]
pub(crate) fn search_directory(
    root: &Path,
    query: &str,
    mode: SearchMode,
    cache: &ContentCache,
) -> Result<SearchResponse, String> {
    let mut results = Vec::new();
    search_streaming(root, query, mode, cache, &AtomicBool::new(false), &mut |batch| {
        results.extend(batch)
    })?;
    Ok(SearchResponse { results })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lite-explorer-search-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = ::tar::Builder::new(Vec::new());
        for (name, body) in files {
            let mut header = ::tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *body).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn snippets_keep_the_match_visible_in_long_lines() {
        let line = format!("{} Zarpa {}", "x".repeat(400), "y".repeat(400));
        let found = matching_line(&line, "zarpa").unwrap();
        assert!(
            found.starts_with('…') && found.ends_with('…') && found.contains("Zarpa"),
            "{found}"
        );
        assert_eq!(match_char_index("İİ zarpa", "zarpa"), Some(3));
    }

    fn write_zip(path: &Path, parts: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in parts {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn content_search_looks_inside_archives_and_documents() {
        let root = temp_dir("content");
        fs::write(root.join("plain.csv"), "id,name\n1,Zarpa API\n").unwrap();
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "a\r\nwindows zarpa\r\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        fs::write(root.join("export.txt"), utf16).unwrap();
        fs::write(
            root.join("notes.rtf"),
            br"{\rtf1{\fonttbl{\f0 Zarpa Sans;}}\f0 Rich {\b zarpa} text\par}",
        )
        .unwrap();
        fs::write(root.join("server.log.gz"), gzip(b"boot\nzarpa started\n")).unwrap();
        fs::write(root.join("image.png.gz"), gzip(b"\x89PNG\0\0zarpa")).unwrap();
        fs::write(
            root.join("backup.tar.zst"),
            ruzstd::encoding::compress_to_vec(
                &tar(&[("etc/app.conf", b"name = zarpa\n")])[..],
                ruzstd::encoding::CompressionLevel::Fastest,
            ),
        )
        .unwrap();
        let mut xz = liblzma::write::XzEncoder::new(Vec::new(), 6);
        xz.write_all(&tar(&[("readme.md", b"# Zarpa xz\n")]))
            .unwrap();
        fs::write(root.join("src.tar.xz"), xz.finish().unwrap()).unwrap();
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        bz.write_all(&tar(&[("a.txt", b"zarpa bz2\n")])).unwrap();
        fs::write(root.join("old.tbz2"), bz.finish().unwrap()).unwrap();
        write_zip(
            &root.join("plugin.jar"),
            &[("META-INF/MANIFEST.MF", b"Implementation-Title: Zarpa\n")],
        );
        let docx = root.join("nested.docx");
        write_zip(
            &docx,
            &[("word/document.xml", b"<w:p><w:t>Zarpa report</w:t></w:p>")],
        );
        write_zip(
            &root.join("bundle.zip"),
            &[
                ("image.bin", b"\0\0zarpa"),
                ("docs/notes.txt", b"intro\nsee Zarpa here\n"),
                ("docs/spec.docx", &fs::read(&docx).unwrap()),
            ],
        );
        write_zip(&root.join("other.zip"), &[("readme.txt", b"nothing")]);

        let mut results = search_directory(&root, "ZARPA", SearchMode::Content, &ContentCache::default())
            .unwrap()
            .results;
        results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let found: Vec<_> = results
            .iter()
            .map(|r| {
                (
                    r.relative_path.as_str(),
                    r.inner_path.as_deref(),
                    r.snippet.as_deref().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("backup.tar.zst", Some("etc/app.conf"), "name = zarpa"),
                ("bundle.zip", Some("docs/notes.txt"), "see Zarpa here"),
                ("export.txt", None, "windows zarpa"),
                ("nested.docx", None, "Zarpa report"),
                ("notes.rtf", None, "Rich zarpa text"),
                ("old.tbz2", Some("a.txt"), "zarpa bz2"),
                ("plain.csv", None, "1,Zarpa API"),
                (
                    "plugin.jar",
                    Some("META-INF/MANIFEST.MF"),
                    "Implementation-Title: Zarpa"
                ),
                ("server.log.gz", None, "zarpa started"),
                ("src.tar.xz", Some("readme.md"), "# Zarpa xz"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fuzzy_search_matches_folders() {
        let root = temp_dir("fuzzy");
        fs::create_dir_all(root.join("docs/invoices")).unwrap();
        fs::write(root.join("docs/invoices/march.pdf"), b"").unwrap();
        fs::write(root.join("docs/old-invoices.csv"), b"").unwrap();
        fs::write(root.join("readme.md"), b"").unwrap();

        let results = search_directory(&root, "invoices", SearchMode::Fuzzy, &ContentCache::default())
            .unwrap()
            .results;
        let mut found: Vec<_> = results
            .iter()
            .map(|r| (r.relative_path.replace('\\', "/"), r.entry.is_directory))
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                ("docs/invoices".to_string(), true),
                ("docs/old-invoices.csv".to_string(), false),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finds_line_case_insensitively() {
        assert_eq!(
            matching_line("a\n  \"title\": \"Zarpa API\",\nb", "\"title\": \"zarpa").as_deref(),
            Some("\"title\": \"Zarpa API\",")
        );
        assert_eq!(matching_line("one\ntwo", "three"), None);
    }

    #[test]
    fn handles_text_whose_lowercase_changes_length() {
        // `İ` lowercases to two chars, which shifted offsets and could panic on a char boundary.
        let text = "İİİİ\nnoise é\nmatch here";
        assert_eq!(matching_line(text, "match").as_deref(), Some("match here"));
        assert_eq!(
            matching_line("İstanbul é", "é").as_deref(),
            Some("İstanbul é")
        );
    }
}
