use std::{
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use crate::{epoch_millis, explorer::archive::Flow, DirectoryEntry};
use serde::{Deserialize, Serialize};

use super::containers::{ArchiveKind, Codec};
use super::documents::DocumentKind;
use super::mail::MailKind;
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
const WORKERS: usize = 4;

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

#[derive(Serialize)]
pub(crate) struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub skipped: usize,
    pub limited: bool,
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

/// Every path under `root`, skipping symlinks. Directories are included only when asked:
/// content search reads files, name search also matches folders.
fn walk(root: &Path, include_directories: bool) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let mut queue = vec![root.to_path_buf()];
    let mut found = Vec::new();
    while let Some(dir) = queue.pop() {
        // Protected or unreadable folders (for example TCC-guarded paths without
        // Full Disk Access) should be skipped, not abort the whole search.
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for child in read.flatten() {
            let path = child.path();
            if child.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
                continue;
            }
            if path.is_dir() {
                if include_directories {
                    found.push(path.clone());
                }
                queue.push(path);
            } else {
                found.push(path);
            }
        }
    }
    Ok(found)
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

/// Searches one file for `needle` (already lowercased). `Err` marks a skipped file: too large,
/// binary or unreadable.
fn content_match(path: PathBuf, needle: &str) -> Result<Option<ContentHit>, ()> {
    let len = fs::metadata(&path).map_err(|_| ())?.len();
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
            bytes_match(&kind, &read_limited(open()?, limit)?, needle)?
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

pub(crate) fn search_directory(
    root: &Path,
    query: &str,
    mode: SearchMode,
) -> Result<SearchResponse, String> {
    let paths = walk(root, matches!(mode, SearchMode::Fuzzy))?;
    let mut skipped = 0;
    let mut results = Vec::new();
    if matches!(mode, SearchMode::Fuzzy) {
        for path in paths {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            // Match the name only, so a folder match does not pull in everything inside it.
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if matches(&name, query) {
                if let Some(entry) = entry(&path) {
                    results.push(SearchResult {
                        entry,
                        relative_path: relative,
                        snippet: None,
                        inner_path: None,
                    });
                }
            }
            if results.len() == MAX_RESULTS {
                break;
            }
        }
    } else {
        let (jobs_tx, jobs_rx) = mpsc::channel::<PathBuf>();
        let jobs_rx = std::sync::Arc::new(std::sync::Mutex::new(jobs_rx));
        let (out_tx, out_rx) = mpsc::channel();
        thread::scope(|scope| {
            for _ in 0..WORKERS {
                let rx = jobs_rx.clone();
                let tx = out_tx.clone();
                let needle = query.to_lowercase();
                scope.spawn(move || {
                    while let Ok(path) = rx.lock().unwrap().recv() {
                        let _ = tx.send(content_match(path, &needle));
                    }
                });
            }
            drop(out_tx);
            for path in paths {
                let _ = jobs_tx.send(path);
            }
            drop(jobs_tx);
            for item in out_rx {
                match item {
                    Ok(Some((path, snippet, inner_path))) => {
                        if results.len() < MAX_RESULTS {
                            if let Some(entry) = entry(&path) {
                                let relative = path
                                    .strip_prefix(root)
                                    .unwrap_or(&path)
                                    .to_string_lossy()
                                    .into_owned();
                                results.push(SearchResult {
                                    entry,
                                    relative_path: relative,
                                    snippet: Some(snippet),
                                    inner_path,
                                });
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(()) => skipped += 1,
                }
            }
        });
    }
    let limited = results.len() >= MAX_RESULTS;
    results.truncate(MAX_RESULTS);
    Ok(SearchResponse {
        results,
        skipped,
        limited,
    })
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

        let mut results = search_directory(&root, "ZARPA", SearchMode::Content)
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

        let results = search_directory(&root, "invoices", SearchMode::Fuzzy)
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
