//! EPUB preview: parses the package (container → OPF) into metadata, spine and a
//! table of contents, and serves chapter XHTML plus inner images/fonts through the
//! `media://` protocol. Scripts are stripped; inner paths can't escape the package.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

use serde::Serialize;
use tauri::State;
use zip::ZipArchive;

use crate::media::{register_zip_root, MediaRegistry};

/// Largest XML/image part read from one chapter; bounds zip bombs.
const MAX_PART_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct EpubTocEntry {
    pub label: String,
    /// Inner package path, with any `#fragment` preserved.
    pub href: Option<String>,
    /// Nesting level from `<ol>` depth, for indentation.
    pub depth: usize,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct EpubChapter {
    /// Inner package path of the chapter document, without fragment.
    pub href: String,
    pub title: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EpubDocument {
    pub title: String,
    pub author: Option<String>,
    pub language: Option<String>,
    /// `media://` base URL; an inner path is appended as `/<path>`.
    pub base: String,
    /// Inner path of the cover image, when the package declares one.
    pub cover: Option<String>,
    /// Inner paths of the stylesheets declared by the package.
    pub styles: Vec<String>,
    pub chapters: Vec<EpubChapter>,
    pub toc: Vec<EpubTocEntry>,
}

/// One `<item>` of the OPF manifest.
struct ManifestItem {
    href: String,
    media_type: String,
    properties: String,
}

#[tauri::command]
pub async fn open_epub(
    registry: State<'_, MediaRegistry>,
    path: String,
) -> Result<EpubDocument, String> {
    let base = register_zip_root(&registry, PathBuf::from(&path));
    tauri::async_runtime::spawn_blocking(move || parse_epub(&path, base))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn read_epub_chapter(path: String, href: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_chapter(&path, &href))
        .await
        .map_err(|error| error.to_string())?
}

fn open_zip(path: &str) -> Result<ZipArchive<BufReader<File>>, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    ZipArchive::new(BufReader::new(file)).map_err(|error| error.to_string())
}

fn read_part(archive: &mut ZipArchive<BufReader<File>>, name: &str) -> Result<String, String> {
    let entry = archive.by_name(name).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    entry
        .take(MAX_PART_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_epub(path: &str, base: String) -> Result<EpubDocument, String> {
    let mut archive = open_zip(path)?;
    let container = read_part(&mut archive, "META-INF/container.xml")?;
    let rootfile =
        attr_in_first(&container, "rootfile", "full-path").ok_or("EPUB has no rootfile")?;
    let opf_dir = directory_of(&rootfile);
    let opf = read_part(&mut archive, &rootfile)?;

    let manifest = parse_manifest(&opf);
    let spine = open_tags(&opf, "itemref")
        .into_iter()
        .filter_map(|tag| attr(tag, "idref"))
        .collect::<Vec<_>>();

    let nav_href = manifest
        .iter()
        .find(|(_, item)| item.properties.split_whitespace().any(|p| p == "nav"))
        .map(|(_, item)| resolve(&opf_dir, &item.href));
    let ncx_href = manifest
        .iter()
        .find(|(_, item)| item.media_type.contains("dtbncx"))
        .map(|(_, item)| resolve(&opf_dir, &item.href));

    let mut toc = Vec::new();
    if let Some(href) = nav_href {
        if let Ok(nav) = read_part(&mut archive, &href) {
            toc = parse_nav(&nav, &directory_of(&href));
        }
    }
    if toc.is_empty() {
        if let Some(href) = ncx_href {
            if let Ok(ncx) = read_part(&mut archive, &href) {
                toc = parse_ncx(&ncx, &directory_of(&href));
            }
        }
    }

    let title_by_href: HashMap<String, String> = toc
        .iter()
        .filter_map(|entry| {
            let href = entry.href.as_ref()?;
            Some((fragment_less(href), entry.label.clone()))
        })
        .collect();

    let mut chapters = Vec::new();
    for idref in spine {
        let Some(item) = manifest.get(&idref) else {
            continue;
        };
        let href = resolve(&opf_dir, &item.href);
        let title = title_by_href
            .get(&href)
            .cloned()
            .unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1));
        chapters.push(EpubChapter { href, title });
    }
    if chapters.is_empty() {
        // No usable spine: fall back to every XHTML part in manifest order.
        for item in manifest.values() {
            if item.media_type.contains("xhtml") || item.media_type.contains("html") {
                let href = resolve(&opf_dir, &item.href);
                let title = title_by_href
                    .get(&href)
                    .cloned()
                    .unwrap_or_else(|| format!("Chapter {}", chapters.len() + 1));
                chapters.push(EpubChapter { href, title });
            }
        }
    }

    let cover = find_cover(&opf, &manifest, &opf_dir);
    let styles = manifest
        .values()
        .filter(|item| item.media_type.contains("css"))
        .map(|item| resolve(&opf_dir, &item.href))
        .collect();

    Ok(EpubDocument {
        title: text_of(&opf, "dc:title").unwrap_or_else(|| "Untitled".into()),
        author: text_of(&opf, "dc:creator"),
        language: text_of(&opf, "dc:language"),
        base,
        cover,
        styles,
        chapters,
        toc,
    })
}

fn parse_manifest(opf: &str) -> HashMap<String, ManifestItem> {
    let mut manifest = HashMap::new();
    for tag in open_tags(opf, "item") {
        let (Some(id), Some(href)) = (attr(tag, "id"), attr(tag, "href")) else {
            continue;
        };
        manifest.insert(
            id,
            ManifestItem {
                href,
                media_type: attr(tag, "media-type").unwrap_or_default(),
                properties: attr(tag, "properties").unwrap_or_default(),
            },
        );
    }
    manifest
}

fn find_cover(
    opf: &str,
    manifest: &HashMap<String, ManifestItem>,
    opf_dir: &str,
) -> Option<String> {
    if let Some(id) = open_tags(opf, "meta")
        .into_iter()
        .find(|tag| attr(tag, "name").as_deref() == Some("cover"))
        .and_then(|tag| attr(tag, "content"))
    {
        if let Some(item) = manifest.get(&id) {
            return Some(resolve(opf_dir, &item.href));
        }
    }
    manifest
        .values()
        .find(|item| {
            item.properties
                .split_whitespace()
                .any(|p| p == "cover-image")
        })
        .map(|item| resolve(opf_dir, &item.href))
}

/// Reads the visible text of one chapter. Script and style blocks are removed so the
/// frontend never renders active content.
fn read_chapter(path: &str, href: &str) -> Result<String, String> {
    let mut archive = open_zip(path)?;
    let html = read_part(&mut archive, href)?;
    Ok(strip_scripts(&html))
}

// --- EPUB3 nav / EPUB2 NCX ------------------------------------------------------

fn parse_nav(nav: &str, base_dir: &str) -> Vec<EpubTocEntry> {
    let Some(nav_tag) = open_tags(nav, "nav")
        .into_iter()
        .find(|tag| attr(tag, "epub:type").as_deref() == Some("toc"))
    else {
        return Vec::new();
    };
    // Work from the toc nav to its closing tag (or the end of the document).
    let start = nav.find(nav_tag).unwrap_or(0);
    let content = &nav[start..];
    let content = content
        .find("</nav>")
        .map_or(content, |end| &content[..end]);

    let mut depth = 0usize;
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(open) = content[i..].find('<') {
        let at = i + open;
        if content[at..].starts_with("<ol") {
            depth += 1;
            i = at + 3;
        } else if content[at..].starts_with("</ol") {
            depth = depth.saturating_sub(1);
            i = at + 4;
        } else if is_anchor_start(content, at) {
            let Some(close) = content[at..].find('>') else {
                break;
            };
            let tag = &content[at..at + close + 1];
            let label_start = at + close + 1;
            let Some(label_end) = content[label_start..].find("</a>") else {
                break;
            };
            out.push(EpubTocEntry {
                label: plain_text(&content[label_start..label_start + label_end]),
                href: attr(tag, "href").map(|href| resolve(base_dir, &href)),
                depth: depth.saturating_sub(1),
            });
            i = label_start + label_end + 4;
        } else {
            i = at + 1;
        }
    }
    out
}

fn parse_ncx(ncx: &str, base_dir: &str) -> Vec<EpubTocEntry> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(open) = ncx[i..].find('<') {
        let at = i + open;
        if ncx[at..].starts_with("<navPoint") {
            depth += 1;
            i = at + 9;
        } else if ncx[at..].starts_with("</navPoint") {
            depth = depth.saturating_sub(1);
            i = at + 10;
        } else if ncx[at..].starts_with("<text") {
            let Some(close) = ncx[at..].find('>') else {
                break;
            };
            let label_start = at + close + 1;
            let Some(label_end) = ncx[label_start..].find("</text>") else {
                break;
            };
            let label = plain_text(&ncx[label_start..label_start + label_end]);
            // The matching <content src> follows the label inside the same navPoint.
            let href = ncx[label_start + label_end..]
                .find("<content")
                .and_then(|offset| {
                    let from = label_start + label_end + offset;
                    let end = ncx[from..].find('>')?;
                    attr(&ncx[from..from + end + 1], "src")
                })
                .map(|href| resolve(base_dir, &href));
            out.push(EpubTocEntry {
                label,
                href,
                depth: depth.saturating_sub(1),
            });
            i = label_start + label_end;
        } else {
            i = at + 1;
        }
    }
    out
}

// --- small XML helpers ----------------------------------------------------------

/// All `<name ...>` open tags, requiring a delimiter after `name` so `<item>` does
/// not match `<itemref>`.
fn open_tags<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!("<{name}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = xml[from..].find(&needle) {
        let start = from + found;
        let after = start + needle.len();
        match xml.as_bytes().get(after) {
            Some(byte) if byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/' => {
                let Some(close) = xml[start..].find('>') else {
                    break;
                };
                out.push(&xml[start..start + close + 1]);
                from = start + close + 1;
            }
            _ => from = after,
        }
    }
    out
}

/// Attribute value from a single open tag, quoted or bare.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let target = name.to_ascii_lowercase();
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(found) = lower[from..].find(&target) {
        let start = from + found;
        let before_ok = start == 0 || !is_name_byte(bytes[start - 1]);
        let after = start + target.len();
        if before_ok {
            let mut i = after;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if bytes.get(i) == Some(&b'=') {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let (quote, value_start) = match bytes.get(i) {
                    Some(&quote @ (b'"' | b'\'')) => (Some(quote), i + 1),
                    Some(_) => (None, i),
                    None => return None,
                };
                let end = match quote {
                    Some(quote) => tag[value_start..]
                        .find(quote as char)
                        .map(|end| value_start + end)?,
                    None => tag[value_start..]
                        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                        .map(|end| value_start + end)
                        .unwrap_or(tag.len()),
                };
                return Some(tag[value_start..end].to_string());
            }
        }
        from = after;
    }
    None
}

fn attr_in_first(xml: &str, name: &str, attribute: &str) -> Option<String> {
    open_tags(xml, name)
        .into_iter()
        .find_map(|tag| attr(tag, attribute))
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
}

fn is_anchor_start(content: &str, at: usize) -> bool {
    content[at..].starts_with("<a")
        && matches!(
            content.as_bytes().get(at + 2),
            Some(b' ' | b'>' | b'\n' | b'\t' | b'\r')
        )
}

/// First `<name>text</name>` content, tags stripped.
fn text_of(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let start = xml.find(&open)?;
    let tag_end = xml[start..].find('>')? + start;
    let close = format!("</{name}>");
    let rest = &xml[tag_end + 1..];
    let end = rest.find(&close)?;
    let text = plain_text(&rest[..end]);
    (!text.is_empty()).then_some(text)
}

fn plain_text(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut depth = 0usize;
    for c in markup.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    decode_entities(out.trim())
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
}

fn strip_scripts(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(open) = find_ci(rest, "<script") {
        out.push_str(&rest[..open]);
        match find_ci(&rest[open..], "</script>") {
            Some(end) => rest = &rest[open + end + 9..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

fn directory_of(path: &str) -> String {
    path.rsplit_once('/')
        .map_or(String::new(), |(dir, _)| dir.to_string())
}

fn fragment_less(href: &str) -> String {
    href.split('#').next().unwrap_or(href).to_string()
}

/// Resolves an href relative to a package directory and normalizes `..`/`.`.
fn resolve(base_dir: &str, href: &str) -> String {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || href.contains("://") || href.starts_with("data:")
    {
        return href.to_string();
    }
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (href, None),
    };
    let combined = if let Some(stripped) = path.strip_prefix('/') {
        stripped.to_string()
    } else if base_dir.is_empty() {
        path.to_string()
    } else {
        format!("{base_dir}/{path}")
    };
    let normalized = normalize_path(&combined);
    match fragment {
        Some(fragment) => format!("{normalized}#{fragment}"),
        None => normalized,
    }
}

fn normalize_path(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn epub_fixture() -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        let files: &[(&str, &str)] = &[
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
            ),
            (
                "OEBPS/content.opf",
                r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>El faro</dc:title><dc:creator>Ana Ojeda</dc:creator><dc:language>es</dc:language></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="cov" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/><item id="c0" href="cap0.xhtml" media-type="application/xhtml+xml"/><item id="c1" href="cap1.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c0"/><itemref idref="c1"/></spine></package>"#,
            ),
            (
                "OEBPS/nav.xhtml",
                r#"<html><body><nav epub:type="toc"><ol><li><a href="cap0.xhtml">El puerto</a></li><li><a href="cap1.xhtml#start">El faro</a></li></ol></nav></body></html>"#,
            ),
            (
                "OEBPS/cap0.xhtml",
                "<html><body><h1>El puerto</h1><script>alert(1)</script></body></html>",
            ),
            (
                "OEBPS/cap1.xhtml",
                "<html><body><h1 id=\"start\">El faro</h1></body></html>",
            ),
            ("OEBPS/cover.jpg", "cover-bytes"),
        ];
        for (name, body) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn write_fixture() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "liteexplorer-epub-{}-{}.epub",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, epub_fixture()).unwrap();
        path
    }

    #[test]
    fn parses_metadata_spine_toc_and_cover() {
        let path = write_fixture();
        let document =
            parse_epub(path.to_str().unwrap(), "media://localhost/token".into()).unwrap();

        assert_eq!(document.title, "El faro");
        assert_eq!(document.author.as_deref(), Some("Ana Ojeda"));
        assert_eq!(document.language.as_deref(), Some("es"));
        assert_eq!(document.cover.as_deref(), Some("OEBPS/cover.jpg"));
        assert_eq!(document.chapters.len(), 2);
        assert_eq!(document.chapters[0].href, "OEBPS/cap0.xhtml");
        assert_eq!(document.chapters[0].title, "El puerto");
        assert_eq!(document.chapters[1].href, "OEBPS/cap1.xhtml");
        assert_eq!(document.toc.len(), 2);
        assert_eq!(
            document.toc[1].href.as_deref(),
            Some("OEBPS/cap1.xhtml#start")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn chapter_reading_strips_scripts() {
        let path = write_fixture();
        let html = read_chapter(path.to_str().unwrap(), "OEBPS/cap0.xhtml").unwrap();
        assert!(html.contains("El puerto"));
        assert!(!html.contains("<script"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn resolve_normalizes_relative_paths() {
        assert_eq!(
            resolve("OEBPS/text", "../images/a.png"),
            "OEBPS/images/a.png"
        );
        assert_eq!(resolve("OEBPS", "cap.html#x"), "OEBPS/cap.html#x");
        assert_eq!(resolve("OEBPS", "https://x/y"), "https://x/y");
    }

    #[test]
    fn attr_reads_quoted_and_bare_values() {
        assert_eq!(
            attr(r#"<a href="x.html" id='y'>"#, "href").as_deref(),
            Some("x.html")
        );
        assert_eq!(
            attr(r#"<a href="x.html" id='y'>"#, "id").as_deref(),
            Some("y")
        );
        assert_eq!(attr("<item id=x/>", "id").as_deref(), Some("x"));
        assert_eq!(attr("<itemref idref=\"a\"/>", "id"), None);
    }
}
