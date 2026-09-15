//! Text extraction for zipped XML documents: Office Open XML (`.docx`, `.xlsx`, `.pptx`)
//! and OpenDocument (`.odt`, `.ods`, `.odp`, ...). Only the parts that hold visible text
//! are read, and markup is reduced to lines so a match yields a readable snippet.

use std::io::{Read, Seek};

use zip::ZipArchive;

use super::matching_line;

/// Largest uncompressed XML part read from one document; bounds zip bombs.
const MAX_PART_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum DocumentKind {
    Word,
    Spreadsheet,
    Presentation,
    OpenDocument,
}

/// Document kind by file name (any case), or `None` for other files.
pub(super) fn kind_for(name: &str) -> Option<DocumentKind> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "docx" | "docm" | "dotx" | "dotm" => Some(DocumentKind::Word),
        "xlsx" | "xlsm" | "xltx" | "xltm" => Some(DocumentKind::Spreadsheet),
        "pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" => Some(DocumentKind::Presentation),
        "odt" | "ott" | "ods" | "ots" | "odp" | "otp" | "odg" | "otg" => Some(DocumentKind::OpenDocument),
        _ => None,
    }
}

fn is_text_part(kind: DocumentKind, name: &str) -> bool {
    match kind {
        DocumentKind::Word => name.strip_prefix("word/").is_some_and(|part| {
            part.ends_with(".xml")
                && ["document", "header", "footer", "footnotes", "endnotes", "comments"]
                    .iter()
                    .any(|prefix| part.starts_with(prefix))
        }),
        // Cell text lives in the shared strings table; worksheet `<v>` values are indexes into it.
        DocumentKind::Spreadsheet => name == "xl/sharedStrings.xml",
        DocumentKind::Presentation => {
            (name.starts_with("ppt/slides/slide") || name.starts_with("ppt/notesSlides/notesSlide"))
                && name.ends_with(".xml")
        }
        DocumentKind::OpenDocument => name == "content.xml",
    }
}

/// First text line of the document containing `needle` (already lowercased).
/// `Err` when the file is not a readable zip.
pub(super) fn document_match<R: Read + Seek>(reader: R, kind: DocumentKind, needle: &str) -> Result<Option<String>, ()> {
    let mut zip = ZipArchive::new(reader).map_err(|_| ())?;
    for index in 0..zip.len() {
        let Ok(part) = zip.by_index(index) else { continue };
        if !is_text_part(kind, part.name()) {
            continue;
        }
        let mut xml = Vec::new();
        if part.take(MAX_PART_BYTES).read_to_end(&mut xml).is_err() {
            continue;
        }
        if let Some(line) = matching_line(&xml_text(&String::from_utf8_lossy(&xml)), needle) {
            return Ok(Some(line));
        }
    }
    Ok(None)
}

/// Strips markup, ending a line at each paragraph, shared string or table row and
/// separating table cells with tabs.
fn xml_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 4);
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        push_decoded(&mut out, &rest[..open]);
        let Some(length) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + length];
        rest = &rest[open + length + 1..];
        let closing = tag.starts_with('/');
        let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or(name);
        match local {
            "p" | "h" | "si" | "tr" | "table-row" if closing => out.push('\n'),
            "tc" | "table-cell" if closing => out.push('\t'),
            "br" | "line-break" if !closing => out.push('\n'),
            "tab" | "s" if !closing => out.push(' '),
            _ => {}
        }
    }
    push_decoded(&mut out, rest);
    out
}

fn push_decoded(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| decode_entity(&rest[1..end]).map(|c| (c, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = entity.strip_prefix('#')?;
            let code = match number.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn zip_bytes(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in parts {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn detects_kinds_by_extension() {
        assert_eq!(kind_for("Report.DOCX"), Some(DocumentKind::Word));
        assert_eq!(kind_for("a.ods"), Some(DocumentKind::OpenDocument));
        assert_eq!(kind_for("data.csv"), None);
        assert_eq!(kind_for("xlsx"), None);
    }

    #[test]
    fn extracts_paragraphs_cells_and_entities() {
        let xml = r#"<?xml version="1.0"?><w:body><w:p><w:r><w:t>Tom &amp; Jerry</w:t></w:r><w:r><w:tab/><w:t>&#233;t&#xE9;</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:t>A1</w:t></w:p></w:tc><w:tc><w:p><w:t>B1</w:t></w:p></w:tc></w:tr></w:tbl></w:body>"#;
        let text = xml_text(xml);
        assert!(text.contains("Tom & Jerry été\n"), "{text:?}");
        assert!(text.contains("A1\n\tB1\n"), "{text:?}");
    }

    #[test]
    fn searches_word_spreadsheet_and_opendocument_parts() {
        let docx = zip_bytes(&[
            ("word/styles.xml", "<w:t>Invoice style</w:t>"),
            ("word/document.xml", "<w:p><w:t>Total due: </w:t><w:t>Invoice 42</w:t></w:p>"),
        ]);
        assert_eq!(document_match(Cursor::new(docx), DocumentKind::Word, "invoice").unwrap().as_deref(), Some("Total due: Invoice 42"));

        let xlsx = zip_bytes(&[("xl/sharedStrings.xml", "<sst><si><t>Name</t></si><si><t>Zarpa API</t></si></sst>")]);
        assert_eq!(document_match(Cursor::new(xlsx), DocumentKind::Spreadsheet, "zarpa").unwrap().as_deref(), Some("Zarpa API"));

        let ods = zip_bytes(&[("content.xml", "<table:table-row><table:table-cell><text:p>x</text:p></table:table-cell></table:table-row>")]);
        assert_eq!(document_match(Cursor::new(ods), DocumentKind::OpenDocument, "missing").unwrap(), None);

        assert!(document_match(Cursor::new(b"not a zip".to_vec()), DocumentKind::Word, "x").is_err());
    }
}
