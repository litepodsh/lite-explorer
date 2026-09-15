use std::{fs, io::{Cursor, Read}, path::{Path, PathBuf}, sync::mpsc, thread};

use serde::{Deserialize, Serialize};
use crate::{archive::{self, Flow}, epoch_millis, DirectoryEntry};

mod documents;

const MAX_RESULTS: usize = 1_000;
const MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024;
/// Office and OpenDocument files are compressed, so they get a larger budget than plain text.
const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
/// Archives are scanned entry by entry; larger ones are skipped to keep a search responsive.
const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const WORKERS: usize = 4;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SearchMode { Fuzzy, Content }

#[derive(Serialize)]
pub(crate) struct SearchResult {
    #[serde(flatten)] pub entry: DirectoryEntry,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")] pub snippet: Option<String>,
    /// Path of the matching file inside an archive result, e.g. `docs/readme.txt`.
    #[serde(skip_serializing_if = "Option::is_none")] pub inner_path: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct SearchResponse { pub results: Vec<SearchResult>, pub skipped: usize, pub limited: bool }

fn matches(haystack: &str, query: &str) -> bool {
    if query.contains(['*', '?']) { return glob_matches(&haystack.to_lowercase(), &query.to_lowercase()); }
    let lower = haystack.to_lowercase();
    let mut chars = lower.chars();
    query.to_lowercase().chars().all(|needle| chars.any(|c| c == needle))
}

fn glob_matches(text: &str, pattern: &str) -> bool {
    let (text, pattern) = (text.as_bytes(), pattern.as_bytes());
    let (mut ti, mut pi, mut star, mut retry) = (0, 0, None, 0);
    while ti < text.len() {
        if pi < pattern.len() && (pattern[pi] == b'?' || pattern[pi] == text[ti]) { ti += 1; pi += 1; }
        else if pi < pattern.len() && pattern[pi] == b'*' { star = Some(pi); pi += 1; retry = ti; }
        else if let Some(at) = star { pi = at + 1; retry += 1; ti = retry; }
        else { return false; }
    }
    while pi < pattern.len() && pattern[pi] == b'*' { pi += 1; }
    pi == pattern.len()
}

fn entry(path: &Path) -> Option<DirectoryEntry> {
    let metadata = fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(DirectoryEntry { name: name.clone(), path: path.to_string_lossy().into_owned(), is_directory: metadata.is_dir(), is_hidden: name.starts_with('.'), size: (!metadata.is_dir()).then_some(metadata.len()), created: epoch_millis(metadata.created()), kind: None })
}

fn files(root: &Path) -> Result<Vec<PathBuf>, String> {
    if !root.is_dir() { return Err(format!("{} is not a directory", root.display())); }
    let mut queue = vec![root.to_path_buf()]; let mut found = Vec::new();
    while let Some(dir) = queue.pop() {
        for child in fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
            let path = child.path();
            if child.file_type().map(|t| t.is_symlink()).unwrap_or(true) { continue; }
            if path.is_dir() { queue.push(path); } else { found.push(path); }
        }
    }
    Ok(found)
}

/// A content match: matching line, plus the entry path when the match is inside an archive.
type ContentHit = (PathBuf, String, Option<String>);

/// Searches one file for `needle` (already lowercased). `Err` marks a skipped file: too large,
/// binary or unreadable.
fn content_match(path: PathBuf, needle: &str) -> Result<Option<ContentHit>, ()> {
    let len = fs::metadata(&path).map_err(|_| ())?.len();
    if archive::is_archive_path(&path) {
        if len > MAX_ARCHIVE_BYTES { return Err(()); }
        return Ok(archive_match(&path, needle)?.map(|(inner, line)| (path, line, Some(inner))));
    }
    let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let limit = content_limit(&name);
    if len > limit { return Err(()); }
    let mut bytes = Vec::new(); fs::File::open(&path).map_err(|_| ())?.take(limit + 1).read_to_end(&mut bytes).map_err(|_| ())?;
    if bytes.len() as u64 > limit { return Err(()); }
    Ok(bytes_match(&name, bytes, needle)?.map(|line| (path, line, None)))
}

fn content_limit(name: &str) -> u64 { if documents::kind_for(name).is_some() { MAX_DOCUMENT_BYTES } else { MAX_CONTENT_BYTES } }

/// Matches file contents named `name`: document text for Office/OpenDocument files, otherwise
/// the bytes as text. `Err` for binary content.
fn bytes_match(name: &str, bytes: Vec<u8>, needle: &str) -> Result<Option<String>, ()> {
    match documents::kind_for(name) {
        Some(kind) => documents::document_match(Cursor::new(bytes), kind, needle),
        None if bytes.contains(&0) => Err(()),
        None => Ok(matching_line(&String::from_utf8_lossy(&bytes), needle)),
    }
}

/// First entry of a `.zip`/`.tar`/`.tar.gz` archive whose contents match, as (entry path, line).
/// Nested archives and oversized or binary entries are passed over.
fn archive_match(path: &Path, needle: &str) -> Result<Option<(String, String)>, ()> {
    let mut found = None;
    archive::for_each_entry(path, &mut |info, reader| {
        let limit = content_limit(&info.path);
        if info.is_directory || info.skipped || info.size > limit { return Ok(Flow::Continue); }
        let mut bytes = Vec::new();
        if reader.take(limit + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > limit { return Ok(Flow::Continue); }
        if let Ok(Some(line)) = bytes_match(&info.path, bytes, needle) { found = Some((info.path.clone(), line)); return Ok(Flow::Stop); }
        Ok(Flow::Continue)
    }).map_err(|_| ())?;
    Ok(found)
}

/// First line containing `needle` (already lowercased), trimmed to a snippet.
/// Lowercases line by line: lowercasing can change byte lengths (e.g. `İ`), so an offset
/// found in a lowercased copy is not a valid offset into the original text.
fn matching_line(text: &str, needle: &str) -> Option<String> {
    text.lines().find(|line| line.to_lowercase().contains(needle)).map(|line| line.trim().chars().take(220).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lite-explorer-search-{tag}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, parts: &[(&str, &[u8])]) {
        let mut zip = ZipWriter::new(fs::File::create(path).unwrap());
        for (name, body) in parts { zip.start_file(*name, SimpleFileOptions::default()).unwrap(); zip.write_all(body).unwrap(); }
        zip.finish().unwrap();
    }

    #[test]
    fn content_search_looks_inside_archives_and_documents() {
        let root = temp_dir("content");
        fs::write(root.join("plain.csv"), "id,name\n1,Zarpa API\n").unwrap();
        let docx = root.join("nested.docx");
        write_zip(&docx, &[("word/document.xml", b"<w:p><w:t>Zarpa report</w:t></w:p>")]);
        write_zip(&root.join("bundle.zip"), &[
            ("image.bin", b"\0\0zarpa"),
            ("docs/notes.txt", b"intro\nsee Zarpa here\n"),
            ("docs/spec.docx", &fs::read(&docx).unwrap()),
        ]);
        write_zip(&root.join("other.zip"), &[("readme.txt", b"nothing")]);

        let mut results = search_directory(&root, "ZARPA", SearchMode::Content).unwrap().results;
        results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        let found: Vec<_> = results.iter().map(|r| (r.relative_path.as_str(), r.inner_path.as_deref(), r.snippet.as_deref().unwrap())).collect();
        assert_eq!(found, [
            ("bundle.zip", Some("docs/notes.txt"), "see Zarpa here"),
            ("nested.docx", None, "Zarpa report"),
            ("plain.csv", None, "1,Zarpa API"),
        ]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn finds_line_case_insensitively() {
        assert_eq!(matching_line("a\n  \"title\": \"Zarpa API\",\nb", "\"title\": \"zarpa").as_deref(), Some("\"title\": \"Zarpa API\","));
        assert_eq!(matching_line("one\ntwo", "three"), None);
    }

    #[test]
    fn handles_text_whose_lowercase_changes_length() {
        // `İ` lowercases to two chars, which shifted offsets and could panic on a char boundary.
        let text = "İİİİ\nnoise é\nmatch here";
        assert_eq!(matching_line(text, "match").as_deref(), Some("match here"));
        assert_eq!(matching_line("İstanbul é", "é").as_deref(), Some("İstanbul é"));
    }
}

pub(crate) fn search_directory(root: &Path, query: &str, mode: SearchMode) -> Result<SearchResponse, String> {
    let paths = files(root)?; let mut skipped = 0; let mut results = Vec::new();
    if matches!(mode, SearchMode::Fuzzy) {
        for path in paths { let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned(); if matches(&relative, query) { if let Some(entry) = entry(&path) { results.push(SearchResult { entry, relative_path: relative, snippet: None, inner_path: None }); } } if results.len() == MAX_RESULTS { break; } }
    } else {
        let (jobs_tx, jobs_rx) = mpsc::channel::<PathBuf>(); let jobs_rx = std::sync::Arc::new(std::sync::Mutex::new(jobs_rx)); let (out_tx, out_rx) = mpsc::channel();
        thread::scope(|scope| { for _ in 0..WORKERS { let rx = jobs_rx.clone(); let tx = out_tx.clone(); let needle = query.to_lowercase(); scope.spawn(move || while let Ok(path) = rx.lock().unwrap().recv() { let _ = tx.send(content_match(path, &needle)); }); } drop(out_tx); for path in paths { let _ = jobs_tx.send(path); } drop(jobs_tx); for item in out_rx { match item { Ok(Some((path, snippet, inner_path))) => if results.len() < MAX_RESULTS { if let Some(entry) = entry(&path) { let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned(); results.push(SearchResult { entry, relative_path: relative, snippet: Some(snippet), inner_path }); } }, Ok(None) => {}, Err(()) => skipped += 1 } } });
    }
    let limited = results.len() >= MAX_RESULTS; results.truncate(MAX_RESULTS); Ok(SearchResponse { results, skipped, limited })
}
