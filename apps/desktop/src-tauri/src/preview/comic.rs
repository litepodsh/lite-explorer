//! Comic-book preview: `.cbz` archives (a zip of page images). Pages stream
//! through the `media://` protocol by reusing the zip root used for EPUB/Office.
//! Read-only; `.cbr` (RAR) isn't supported.

use std::{cmp::Ordering, fs::File, io::BufReader};

use serde::Serialize;
use tauri::State;
use zip::ZipArchive;

use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::media::{register_zip_root, MediaRegistry};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ComicPreview {
    /// `media://` base URL; a page inner path is appended as `/<path>`.
    pub base: String,
    /// Inner paths of the pages, in reading order.
    pub pages: Vec<String>,
}

pub(crate) fn is_comic_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "cbz")
}

#[tauri::command]
pub async fn open_comic(
    registry: State<'_, MediaRegistry>,
    path: String,
) -> Result<ComicPreview, String> {
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    let base = register_zip_root(&registry, path.clone());
    tauri::async_runtime::spawn_blocking(move || parse_comic(&path.to_string_lossy(), base))
        .await
        .map_err(|error| error.to_string())?
}

fn is_image(name: &str) -> bool {
    matches!(
        name.rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif"
    )
}

/// Orders `page-2.png` before `page-10.png` by comparing digit runs numerically.
fn natural(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let mut xd = String::new();
                    while left.peek().is_some_and(char::is_ascii_digit) {
                        xd.push(left.next().unwrap());
                    }
                    let mut yd = String::new();
                    while right.peek().is_some_and(char::is_ascii_digit) {
                        yd.push(right.next().unwrap());
                    }
                    let xn = xd.trim_start_matches('0').parse::<u64>().unwrap_or(0);
                    let yn = yd.trim_start_matches('0').parse::<u64>().unwrap_or(0);
                    match xn.cmp(&yn) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                }
                left.next();
                right.next();
                match x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase()) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
        }
    }
}

pub(crate) fn parse_comic(path: &str, base: String) -> Result<ComicPreview, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|error| error.to_string())?;
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        if is_image(&name) {
            pages.push(name);
        }
    }
    if pages.is_empty() {
        return Err("No pages found in comic".to_string());
    }
    pages.sort_by(|a, b| natural(a, b));
    Ok(ComicPreview { base, pages })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    #[test]
    fn sorts_pages_naturally() {
        let mut names = vec![
            "p10.png".to_string(),
            "p2.png".to_string(),
            "p1.png".to_string(),
        ];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, vec!["p1.png", "p2.png", "p10.png"]);
    }

    #[test]
    fn lists_only_images() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for name in ["page-2.png", "notes.txt", "page-1.jpg"] {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"x").unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let dir = std::env::temp_dir().join("lite-comic-test.cbz");
        std::fs::write(&dir, bytes).unwrap();
        let comic = parse_comic(dir.to_str().unwrap(), "media://localhost/x".into()).unwrap();
        assert_eq!(comic.pages, vec!["page-1.jpg", "page-2.png"]);
        std::fs::remove_file(dir).unwrap();
    }
}
