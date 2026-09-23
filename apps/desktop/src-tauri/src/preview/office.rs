//! Office preview: renders Word documents (`.docx`, `.odt`, flat `.fodt`) and
//! presentations (`.pptx`, `.odp`, flat `.fodp`) into sanitized HTML. Images are
//! served from the package through the `media://` protocol. Read-only: no editing,
//! and every text node is escaped by construction.

use std::{collections::HashMap, fs::File, io::BufReader, io::Read};

use serde::Serialize;
use tauri::State;
use zip::ZipArchive;

use crate::explorer::local_path::{validate_existing, ExpectedKind};
use crate::media::{register_zip_root, MediaRegistry};

use crate::preview::xml::{self, Element};
#[cfg(test)]
use std::path::PathBuf;

/// Largest XML part read from one package; bounds zip bombs.
const MAX_PART_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WordDocument {
    pub title: Option<String>,
    pub html: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Slide {
    pub title: String,
    pub html: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Presentation {
    pub title: Option<String>,
    pub slides: Vec<Slide>,
}

pub(crate) fn is_word_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "docx" | "docm" | "dotx" | "dotm" | "odt" | "ott" | "fodt"
    )
}

pub(crate) fn is_presentation_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" | "odp" | "otp" | "fodp"
    )
}

fn is_flat(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "fodt" | "fodp")
}

fn extension_of(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

#[tauri::command]
pub async fn open_word(
    registry: State<'_, MediaRegistry>,
    path: String,
) -> Result<WordDocument, String> {
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    let base = register_zip_root(&registry, path.clone());
    tauri::async_runtime::spawn_blocking(move || parse_word(&path.to_string_lossy(), base))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn open_presentation(
    registry: State<'_, MediaRegistry>,
    path: String,
) -> Result<Presentation, String> {
    let path = validate_existing(std::path::Path::new(&path), ExpectedKind::File)?;
    let base = register_zip_root(&registry, path.clone());
    tauri::async_runtime::spawn_blocking(move || parse_presentation(&path.to_string_lossy(), base))
        .await
        .map_err(|error| error.to_string())?
}

// --- package reading ------------------------------------------------------------

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

fn read_optional(archive: &mut ZipArchive<BufReader<File>>, name: &str) -> Option<String> {
    read_part(archive, name).ok()
}

fn read_flat(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    crate::search::text::decode(&bytes)
        .map(|text| text.into_owned())
        .ok_or_else(|| "Unreadable document".to_string())
}

/// Relationship id → target, from a `_rels` part.
fn parse_rels(xml_text: &str) -> HashMap<String, String> {
    let document = xml::parse_document(xml_text);
    let mut rels = HashMap::new();
    for relationship in document
        .descendant("Relationships")
        .map_or_else(Vec::new, |node| {
            node.children_named("Relationship").collect()
        })
    {
        let (Some(id), Some(target)) = (relationship.attr("Id"), relationship.attr("Target"))
        else {
            continue;
        };
        rels.insert(id.to_string(), target.to_string());
    }
    rels
}

fn resolve(base_dir: &str, target: &str) -> String {
    let target = target.trim();
    if let Some(stripped) = target.strip_prefix('/') {
        return xml::normalize_path(stripped);
    }
    if base_dir.is_empty() {
        xml::normalize_path(target)
    } else {
        xml::normalize_path(&format!("{base_dir}/{target}"))
    }
}

// --- Word: .docx ----------------------------------------------------------------

fn parse_word(path: &str, base: String) -> Result<WordDocument, String> {
    let extension = extension_of(path);
    if is_flat(&extension) {
        let document = xml::parse_document(&read_flat(path)?);
        let body = document
            .descendant("text")
            .or_else(|| document.descendant("body"))
            .ok_or("Document has no body")?;
        return Ok(WordDocument {
            title: document
                .descendant("title")
                .map(|title| title.text_content()),
            html: render_odf_blocks(body, &base),
        });
    }

    let mut archive = open_zip(path)?;
    if extension == "odt" || extension == "ott" {
        let content = read_part(&mut archive, "content.xml")?;
        let document = xml::parse_document(&content);
        let body = document
            .descendant("text")
            .or_else(|| document.descendant("body"))
            .ok_or("Document has no body")?;
        return Ok(WordDocument {
            title: None,
            html: render_odf_blocks(body, &base),
        });
    }

    let document = read_part(&mut archive, "word/document.xml")?;
    let rels = read_optional(&mut archive, "word/_rels/document.xml.rels")
        .map(|xml| parse_rels(&xml))
        .unwrap_or_default();
    let root = xml::parse_document(&document);
    let body = root.descendant("body").ok_or("Document has no body")?;
    Ok(WordDocument {
        title: root
            .descendant("coreProperties")
            .and_then(|core| core.descendant("title"))
            .map(|title| title.text_content()),
        html: render_docx_blocks(body, &rels, &base),
    })
}

fn render_docx_blocks(parent: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let mut out = String::new();
    for child in &parent.children {
        match child.local() {
            "p" => out.push_str(&render_docx_paragraph(child, rels, base)),
            "tbl" => out.push_str(&render_docx_table(child, rels, base)),
            "sdt" => {
                if let Some(content) = child.descendant("sdtContent") {
                    out.push_str(&render_docx_blocks(content, rels, base));
                }
            }
            _ => {}
        }
    }
    out
}

fn render_docx_paragraph(p: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let props = p.children_named("pPr").next();
    let style = props
        .and_then(|props| props.children_named("pStyle").next())
        .and_then(|style| style.attr("val"))
        .unwrap_or("");
    let bullet = props
        .and_then(|props| props.children_named("numPr").next())
        .is_some();
    let align = props
        .and_then(|props| props.children_named("jc").next())
        .and_then(|jc| jc.attr("val"))
        .map(alignment)
        .unwrap_or("");

    let mut inner = String::new();
    for child in &p.children {
        match child.local() {
            "r" => inner.push_str(&render_docx_run(child, rels, base)),
            "hyperlink" | "ins" | "smartTag" | "sdt" | "sdtContent" => {
                inner.push_str(&render_docx_inline_children(child, rels, base));
            }
            "fldSimple" => inner.push_str(&render_docx_inline_children(child, rels, base)),
            _ => {}
        }
    }
    if inner.is_empty() && !bullet {
        return String::new();
    }
    if bullet {
        return format!("<p class=\"office-item\">• {inner}</p>");
    }
    let tag = heading_tag(style);
    let style_attr = if align.is_empty() {
        String::new()
    } else {
        format!(" style=\"text-align:{align}\"")
    };
    format!("<{tag}{style_attr}>{inner}</{tag}>")
}

fn render_docx_inline_children(
    parent: &Element,
    rels: &HashMap<String, String>,
    base: &str,
) -> String {
    let mut out = String::new();
    for child in &parent.children {
        match child.local() {
            "r" => out.push_str(&render_docx_run(child, rels, base)),
            "p" => out.push_str(&render_docx_paragraph(child, rels, base)),
            _ => out.push_str(&render_docx_inline_children(child, rels, base)),
        }
    }
    out
}

fn render_docx_run(run: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let props = run.children_named("rPr").next();
    let flag = |name: &str| {
        props
            .and_then(|props| props.children_named(name).next())
            .map(|node| node.attr("val") != Some("0") && node.attr("val") != Some("false"))
            .unwrap_or(false)
    };
    let mut html = String::new();
    for child in &run.children {
        match child.local() {
            "t" => html.push_str(&escape(&child.text_content())),
            "tab" => html.push('\t'),
            "br" | "cr" => html.push_str("<br>"),
            "noBreakHyphen" => html.push('-'),
            "drawing" | "pict" => html.push_str(&render_docx_image(child, rels, base)),
            _ => {}
        }
    }
    if html.is_empty() {
        return html;
    }
    if flag("b") {
        html = format!("<strong>{html}</strong>");
    }
    if flag("i") {
        html = format!("<em>{html}</em>");
    }
    if flag("u") {
        html = format!("<u>{html}</u>");
    }
    if flag("strike") {
        html = format!("<s>{html}</s>");
    }
    let mut style = String::new();
    if let Some(align) = props
        .and_then(|props| props.children_named("vertAlign").next())
        .and_then(|node| node.attr("val"))
    {
        match align {
            "superscript" => style.push_str("vertical-align:super;font-size:smaller;"),
            "subscript" => style.push_str("vertical-align:sub;font-size:smaller;"),
            _ => {}
        }
    }
    if let Some(size) = props
        .and_then(|props| props.children_named("sz").next())
        .and_then(|node| node.attr("val"))
        .and_then(|value| value.parse::<f64>().ok())
    {
        style.push_str(&format!("font-size:{}pt;", size / 2.0));
    }
    if let Some(color) = props
        .and_then(|props| props.children_named("color").next())
        .and_then(|node| node.attr("val"))
    {
        if color != "auto" {
            style.push_str(&format!("color:#{color};"));
        }
    }
    if !style.is_empty() {
        html = format!("<span style=\"{style}\">{html}</span>");
    }
    html
}

fn render_docx_table(table: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let mut html = String::from("<table>");
    for row in table.children_named("tr") {
        html.push_str("<tr>");
        for cell in row.children_named("tc") {
            html.push_str("<td>");
            html.push_str(&render_docx_blocks(cell, rels, base));
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    html
}

fn render_docx_image(node: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let id = node
        .descendant("blip")
        .and_then(|blip| blip.attr("embed").or_else(|| blip.attr("link")))
        .or_else(|| {
            node.descendant("imagedata")
                .and_then(|data| data.attr("id"))
        });
    let Some(target) = id.and_then(|id| rels.get(id)) else {
        return String::new();
    };
    let resolved = resolve("word", target);
    format!("<img src=\"{}/{}\" alt=\"\">", base, encode_path(&resolved))
}

// --- Word: OpenDocument ---------------------------------------------------------

fn render_odf_blocks(parent: &Element, base: &str) -> String {
    let mut out = String::new();
    for child in &parent.children {
        match child.local() {
            "h" => {
                let level = child
                    .attr("outline-level")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1)
                    .clamp(1, 6);
                out.push_str(&format!(
                    "<h{level}>{}</h{level}>",
                    render_odf_inline(child, base)
                ));
            }
            "p" => out.push_str(&format!("<p>{}</p>", render_odf_inline(child, base))),
            "list" => out.push_str(&render_odf_list(child, base)),
            "table" => out.push_str(&render_odf_table(child, base)),
            "section" | "text-body" | "text" | "body" => {
                out.push_str(&render_odf_blocks(child, base));
            }
            _ => {}
        }
    }
    out
}

fn render_odf_list(list: &Element, base: &str) -> String {
    let mut html = String::from("<ul>");
    for item in list.children_named("list-item") {
        html.push_str("<li>");
        html.push_str(&render_odf_blocks(item, base));
        html.push_str("</li>");
    }
    html.push_str("</ul>");
    html
}

fn render_odf_table(table: &Element, base: &str) -> String {
    let mut html = String::from("<table>");
    for row in table.children_named("table-row") {
        html.push_str("<tr>");
        for cell in row.children_named("table-cell") {
            html.push_str("<td>");
            html.push_str(&render_odf_blocks(cell, base));
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</table>");
    html
}

fn render_odf_inline(parent: &Element, base: &str) -> String {
    let mut out = String::new();
    for child in &parent.children {
        match child.local() {
            "#text" => out.push_str(&escape(&child.text)),
            "s" => {
                let count = child
                    .attr("c")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(1);
                for _ in 0..count {
                    out.push(' ');
                }
            }
            "tab" => out.push('\t'),
            "line-break" => out.push_str("<br>"),
            "image" => out.push_str(&render_odf_image(child, base)),
            "a" => out.push_str(&render_odf_inline(child, base)),
            "frame" => out.push_str(&render_odf_inline(child, base)),
            "span" | "text-box" => out.push_str(&render_odf_inline(child, base)),
            _ => out.push_str(&render_odf_inline(child, base)),
        }
    }
    out
}

fn render_odf_image(image: &Element, base: &str) -> String {
    let Some(href) = image.attr("href") else {
        return String::new();
    };
    if href.starts_with("http") || href.starts_with("data:") {
        return String::new();
    }
    let resolved = resolve("", href);
    format!("<img src=\"{}/{}\" alt=\"\">", base, encode_path(&resolved))
}

// --- Presentation: .pptx --------------------------------------------------------

fn parse_presentation(path: &str, base: String) -> Result<Presentation, String> {
    let extension = extension_of(path);
    if is_flat(&extension) {
        let document = xml::parse_document(&read_flat(path)?);
        let body = document
            .descendant("presentation")
            .or_else(|| document.descendant("body"))
            .ok_or("Presentation has no body")?;
        return Ok(Presentation {
            title: document
                .descendant("title")
                .map(|title| title.text_content()),
            slides: render_odf_slides(body, &base),
        });
    }

    let mut archive = open_zip(path)?;
    if extension == "odp" || extension == "otp" {
        let content = read_part(&mut archive, "content.xml")?;
        let document = xml::parse_document(&content);
        let body = document
            .descendant("presentation")
            .or_else(|| document.descendant("body"))
            .ok_or("Presentation has no body")?;
        return Ok(Presentation {
            title: None,
            slides: render_odf_slides(body, &base),
        });
    }

    let presentation = read_part(&mut archive, "ppt/presentation.xml")?;
    let rels = read_optional(&mut archive, "ppt/_rels/presentation.xml.rels")
        .map(|xml| parse_rels(&xml))
        .unwrap_or_default();
    let root = xml::parse_document(&presentation);
    let mut slides = Vec::new();
    for sld in root
        .descendant("sldIdLst")
        .map_or_else(Vec::new, |list| list.children_named("sldId").collect())
    {
        let Some(id) = sld.attr("r:id").or_else(|| sld.attr("id")) else {
            continue;
        };
        let Some(target) = rels.get(id) else {
            continue;
        };
        let slide_path = resolve("ppt", target);
        let slide_xml = match read_optional(&mut archive, &slide_path) {
            Some(xml) => xml,
            None => continue,
        };
        let slide_rels = read_optional(
            &mut archive,
            &format!("{}_rels/{}", directory(&slide_path), file_name(&slide_path)),
        )
        .map(|xml| parse_rels(&xml))
        .unwrap_or_default();
        slides.push(render_pptx_slide(
            &xml::parse_document(&slide_xml),
            &slide_rels,
            &base,
            slides.len() + 1,
        ));
    }
    Ok(Presentation {
        title: None,
        slides,
    })
}

fn render_pptx_slide(
    root: &Element,
    rels: &HashMap<String, String>,
    base: &str,
    index: usize,
) -> Slide {
    let mut title = String::new();
    let mut body = String::new();
    for shape in root
        .descendant("spTree")
        .map_or_else(Vec::new, |tree| tree.children_named("sp").collect())
    {
        let is_title = shape
            .descendant("ph")
            .and_then(|ph| ph.attr("type"))
            .is_some_and(|kind| kind == "title" || kind == "ctrTitle");
        let text = shape
            .descendant("txBody")
            .map(pptx_text)
            .unwrap_or_default();
        if is_title {
            title = text.trim().to_string();
        } else if !text.trim().is_empty() {
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                body.push_str(&format!("<p>{}</p>", escape(line.trim())));
            }
        }
        for picture in shape.children_named("pic") {
            body.push_str(&render_pptx_image(picture, rels, base));
        }
    }
    for picture in root
        .descendant("spTree")
        .map_or_else(Vec::new, |tree| tree.children_named("pic").collect())
    {
        body.push_str(&render_pptx_image(picture, rels, base));
    }
    Slide {
        title: if title.is_empty() {
            format!("Slide {index}")
        } else {
            title
        },
        html: body,
    }
}

fn pptx_text(tx_body: &Element) -> String {
    let mut lines = Vec::new();
    for paragraph in tx_body.children_named("p") {
        lines.push(paragraph.text_content());
    }
    lines.join("\n")
}

fn render_pptx_image(picture: &Element, rels: &HashMap<String, String>, base: &str) -> String {
    let Some(target) = picture
        .descendant("blip")
        .and_then(|blip| blip.attr("embed"))
        .and_then(|id| rels.get(id))
    else {
        return String::new();
    };
    let resolved = resolve("ppt/slides", target);
    format!("<img src=\"{}/{}\" alt=\"\">", base, encode_path(&resolved))
}

// --- Presentation: OpenDocument -------------------------------------------------

fn render_odf_slides(body: &Element, base: &str) -> Vec<Slide> {
    let mut slides = Vec::new();
    for page in body.children_named("page") {
        let mut title = String::new();
        let mut html = String::new();
        let frames: Vec<&Element> = page
            .children_named("frame")
            .filter(|frame| frame.attr("class") != Some("notes"))
            .collect();
        for (index, frame) in frames.iter().enumerate() {
            let class = frame.attr("class").unwrap_or("");
            // OpenDocument decks often omit the title class; the first frame is it.
            if class == "title" || (index == 0 && class != "subtitle") {
                title = frame.text_content().trim().to_string();
                continue;
            }
            for image in frame.children_named("image") {
                html.push_str(&render_odf_image(image, base));
            }
            for text_box in frame.children_named("text-box") {
                for paragraph in text_box.children_named("p") {
                    let line = paragraph.text_content();
                    if !line.trim().is_empty() {
                        html.push_str(&format!("<p>{}</p>", escape(line.trim())));
                    }
                }
            }
        }
        slides.push(Slide {
            title: if title.is_empty() {
                format!("Slide {}", slides.len() + 1)
            } else {
                title
            },
            html,
        });
    }
    slides
}

// --- small helpers --------------------------------------------------------------

fn heading_tag(style: &str) -> &'static str {
    let lower = style.to_ascii_lowercase();
    if lower == "title" {
        return "h1";
    }
    if let Some(digit) = lower
        .strip_prefix("heading")
        .and_then(|rest| rest.parse::<u8>().ok())
    {
        return match digit {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            5 => "h5",
            _ => "h6",
        };
    }
    "p"
}

fn alignment(value: &str) -> &'static str {
    match value {
        "center" => "center",
        "right" | "end" => "right",
        "both" | "justify" | "distribute" => "justify",
        _ => "",
    }
}

fn directory(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(dir, _)| format!("{dir}/"))
        .unwrap_or_default()
}

fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// Percent-encodes each path segment, keeping separators, so the media URL is valid.
fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            segment
                .bytes()
                .map(|byte| match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        (byte as char).to_string()
                    }
                    _ => format!("%{byte:02X}"),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_bytes(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in parts {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "liteexplorer-office-{}-{}-{name}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn detects_word_and_presentation_extensions() {
        assert!(is_word_extension("DOCX"));
        assert!(is_word_extension("odt"));
        assert!(is_word_extension("fodt"));
        assert!(is_presentation_extension("pptx"));
        assert!(is_presentation_extension("odp"));
        assert!(!is_word_extension("doc"));
        assert!(!is_presentation_extension("ppt"));
    }

    #[test]
    fn parses_odt_blocks_and_tables() {
        let content = concat!(
            r#"<?xml version="1.0"?><office:document><office:body><office:text>"#,
            r#"<text:h text:outline-level="1">Título</text:h>"#,
            r#"<text:p>Hola <text:span>mundo</text:span></text:p>"#,
            r#"<table:table><table:table-row>"#,
            r#"<table:table-cell><text:p>A1</text:p></table:table-cell>"#,
            r#"<table:table-cell><text:p>B1</text:p></table:table-cell>"#,
            r#"</table:table-row></table:table>"#,
            r#"</office:text></office:body></office:document>"#,
        );
        let path = write_temp(
            "doc.odt",
            &zip_bytes(&[("content.xml", content), ("mimetype", "x")]),
        );
        let document = parse_word(path.to_str().unwrap(), "media://x/tok".into()).unwrap();
        assert!(document.html.contains("<h1>Título</h1>"));
        assert!(document.html.contains("<p>Hola mundo</p>"));
        assert!(document.html.contains("<td><p>A1</p></td>"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_pptx_slide_order_and_text() {
        let presentation = r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId3"/></p:sldIdLst></p:presentation>"#;
        let rels = r#"<Relationships><Relationship Id="rId3" Target="slides/slide1.xml"/></Relationships>"#;
        let slide = concat!(
            r#"<p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree>"#,
            r#"<p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>"#,
            r#"<p:txBody><a:p><a:r><a:t>Hola</a:t></a:r></a:p></p:txBody></p:sp>"#,
            r#"<p:sp><p:txBody><a:p><a:r><a:t>Cuerpo</a:t></a:r></a:p></p:txBody></p:sp>"#,
            r#"</p:spTree></p:cSld></p:sld>"#,
        );
        let path = write_temp(
            "deck.pptx",
            &zip_bytes(&[
                ("ppt/presentation.xml", presentation),
                ("ppt/_rels/presentation.xml.rels", rels),
                ("ppt/slides/slide1.xml", slide),
            ]),
        );
        let deck = parse_presentation(path.to_str().unwrap(), "media://x/tok".into()).unwrap();
        assert_eq!(deck.slides.len(), 1);
        assert_eq!(deck.slides[0].title, "Hola");
        assert!(deck.slides[0].html.contains("<p>Cuerpo</p>"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn escapes_text_and_encodes_media_paths() {
        assert_eq!(escape("<b>&</b>"), "&lt;b&gt;&amp;&lt;/b&gt;");
        assert_eq!(encode_path("Pictures/a b.png"), "Pictures/a%20b.png");
        assert_eq!(heading_tag("Heading2"), "h2");
        assert_eq!(heading_tag("Title"), "h1");
    }
}
