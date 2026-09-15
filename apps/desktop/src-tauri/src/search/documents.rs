//! Text extraction for documents: Office Open XML (`.docx`, `.pptx`), OpenDocument
//! (`.odt`, `.odp`, flat `.fodt`, ...), EPUB, and spreadsheets (`.xlsx`, `.xls`, `.xlsb`,
//! `.ods`) through calamine. Only the parts that hold visible text are read, and markup is
//! reduced to lines so a match yields a readable snippet.

use std::io::{Cursor, Read, Seek};

use calamine::Reader;
use zip::ZipArchive;

use super::{matching_line, text};

/// Largest uncompressed XML part read from one document; bounds zip bombs.
const MAX_PART_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum DocumentKind {
    Word,
    Presentation,
    OpenDocument,
    Epub,
    Spreadsheet,
    /// Uncompressed single-file OpenDocument XML.
    FlatOpenDocument,
}

/// Document kind by lowercased extension, or `None` for other files.
pub(super) fn kind_for(extension: &str) -> Option<DocumentKind> {
    match extension {
        "docx" | "docm" | "dotx" | "dotm" => Some(DocumentKind::Word),
        "pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" => Some(DocumentKind::Presentation),
        "odt" | "ott" | "odp" | "otp" | "odg" | "otg" => Some(DocumentKind::OpenDocument),
        "epub" => Some(DocumentKind::Epub),
        "xlsx" | "xlsm" | "xltx" | "xltm" | "xlsb" | "xls" | "xla" | "xlam" | "ods" | "ots" => {
            Some(DocumentKind::Spreadsheet)
        }
        "fodt" | "fods" | "fodp" | "fodg" => Some(DocumentKind::FlatOpenDocument),
        _ => None,
    }
}

/// Largest file of each kind content search reads.
pub(super) fn max_bytes(kind: DocumentKind) -> u64 {
    match kind {
        // Spreadsheets are expanded into memory cell by cell.
        DocumentKind::Spreadsheet | DocumentKind::FlatOpenDocument => 32 * 1024 * 1024,
        _ => 64 * 1024 * 1024,
    }
}

/// First text line of the document containing `needle` (already lowercased).
/// `Err` when the file cannot be read as that kind.
pub(super) fn document_match(
    bytes: &[u8],
    kind: DocumentKind,
    needle: &str,
) -> Result<Option<String>, ()> {
    match kind {
        DocumentKind::Spreadsheet => spreadsheet_match(bytes, needle),
        DocumentKind::FlatOpenDocument => Ok(matching_line(
            &xml_text(&text::decode(bytes).ok_or(())?),
            needle,
        )),
        _ => zipped_match(Cursor::new(bytes), kind, needle),
    }
}

/// Matches spreadsheet rows, cells joined with ` | `; numbers and dates are searchable as shown.
fn spreadsheet_match(bytes: &[u8], needle: &str) -> Result<Option<String>, ()> {
    let mut workbook = calamine::open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|_| ())?;
    for (_, range) in workbook.worksheets() {
        for row in range.rows() {
            let cells: Vec<String> = row
                .iter()
                .map(ToString::to_string)
                .filter(|cell| !cell.is_empty())
                .collect();
            if let Some(line) = matching_line(&cells.join(" | "), needle) {
                return Ok(Some(line));
            }
        }
    }
    Ok(None)
}

fn is_text_part(kind: DocumentKind, name: &str) -> bool {
    match kind {
        DocumentKind::Word => name.strip_prefix("word/").is_some_and(|part| {
            part.ends_with(".xml")
                && [
                    "document",
                    "header",
                    "footer",
                    "footnotes",
                    "endnotes",
                    "comments",
                ]
                .iter()
                .any(|prefix| part.starts_with(prefix))
        }),
        DocumentKind::Presentation => {
            (name.starts_with("ppt/slides/slide") || name.starts_with("ppt/notesSlides/notesSlide"))
                && name.ends_with(".xml")
        }
        DocumentKind::OpenDocument => name == "content.xml",
        DocumentKind::Epub => {
            let lower = name.to_ascii_lowercase();
            !lower.starts_with("meta-inf/")
                && [".xhtml", ".html", ".htm"]
                    .iter()
                    .any(|extension| lower.ends_with(extension))
        }
        DocumentKind::Spreadsheet | DocumentKind::FlatOpenDocument => false,
    }
}

fn zipped_match<R: Read + Seek>(
    reader: R,
    kind: DocumentKind,
    needle: &str,
) -> Result<Option<String>, ()> {
    let mut zip = ZipArchive::new(reader).map_err(|_| ())?;
    for index in 0..zip.len() {
        let Ok(part) = zip.by_index(index) else {
            continue;
        };
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

/// Strips XML/XHTML markup, ending a line at each paragraph, heading, list item or table row.
/// Table cells stay on their row's line, separated by ` | `. Script and style contents are dropped.
pub(super) fn xml_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 4);
    let mut rest = xml;
    // Open table cells; paragraphs inside a cell must not break the row's line.
    let mut cell_depth = 0usize;
    while let Some(open) = rest.find('<') {
        push_decoded(&mut out, &rest[..open]);
        let Some(length) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + length];
        rest = &rest[open + length + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        let local = name.rsplit(':').next().unwrap_or(name).to_ascii_lowercase();
        if !closing && !tag.ends_with('/') && (local == "script" || local == "style") {
            let end = format!("</{name}");
            rest = rest.find(&end).map_or("", |at| &rest[at..]);
            continue;
        }
        let self_closing = tag.ends_with('/');
        match local.as_str() {
            "tc" | "td" | "th" | "table-cell" if !closing && !self_closing => cell_depth += 1,
            "tc" | "td" | "th" | "table-cell" if closing => {
                cell_depth = cell_depth.saturating_sub(1);
                out.truncate(out.trim_end_matches(' ').len());
                out.push_str(" | ");
            }
            "tr" | "table-row" if closing => {
                if out.ends_with(" | ") {
                    out.truncate(out.len() - 3);
                }
                out.push('\n');
            }
            "p" | "h" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "div" | "title"
            | "blockquote" | "dt" | "dd" | "si"
                if closing =>
            {
                if cell_depth == 0 {
                    out.push('\n');
                } else if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
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
        let decoded = rest
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| decode_entity(&rest[1..end]).map(|c| (c, end)));
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
        "nbsp" => Some(' '),
        "ndash" => Some('–'),
        "mdash" => Some('—'),
        "hellip" => Some('…'),
        "lsquo" => Some('‘'),
        "rsquo" => Some('’'),
        "ldquo" => Some('“'),
        "rdquo" => Some('”'),
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
    use std::io::Write;
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
        assert_eq!(kind_for("docx"), Some(DocumentKind::Word));
        assert_eq!(kind_for("ods"), Some(DocumentKind::Spreadsheet));
        assert_eq!(kind_for("xls"), Some(DocumentKind::Spreadsheet));
        assert_eq!(kind_for("fodt"), Some(DocumentKind::FlatOpenDocument));
        assert_eq!(kind_for("csv"), None);
    }

    #[test]
    fn extracts_paragraphs_cells_and_entities() {
        let xml = r#"<?xml version="1.0"?><w:body><w:p><w:r><w:t>Tom &amp; Jerry</w:t></w:r><w:r><w:tab/><w:t>&#233;t&#xE9;</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:t>A1</w:t></w:p></w:tc><w:tc><w:p><w:t>B1</w:t></w:p></w:tc></w:tr></w:tbl></w:body>"#;
        let text = xml_text(xml);
        assert!(text.contains("Tom & Jerry été\n"), "{text:?}");
        assert!(text.contains("\nA1 | B1\n"), "{text:?}");
    }

    #[test]
    fn searches_word_spreadsheet_and_opendocument_parts() {
        let docx = zip_bytes(&[
            ("word/styles.xml", "<w:t>Invoice style</w:t>"),
            (
                "word/document.xml",
                "<w:p><w:t>Total due: </w:t><w:t>Invoice 42</w:t></w:p>",
            ),
        ]);
        assert_eq!(
            document_match(&docx, DocumentKind::Word, "invoice")
                .unwrap()
                .as_deref(),
            Some("Total due: Invoice 42")
        );

        let odt = zip_bytes(&[(
            "content.xml",
            "<text:p>Hola <text:span>Zarpa</text:span></text:p>",
        )]);
        assert_eq!(
            document_match(&odt, DocumentKind::OpenDocument, "hola zarpa")
                .unwrap()
                .as_deref(),
            Some("Hola Zarpa")
        );
        assert_eq!(
            document_match(&odt, DocumentKind::OpenDocument, "missing").unwrap(),
            None
        );

        assert!(document_match(b"not a zip", DocumentKind::Word, "x").is_err());
    }

    #[test]
    fn searches_epub_chapters_and_flat_opendocument() {
        let epub = zip_bytes(&[
            ("META-INF/container.xml", "<rootfile>zarpa</rootfile>"),
            ("OEBPS/ch1.xhtml", "<html><head><style>.zarpa{}</style></head><body><h1>Chapter&nbsp;1</h1><p>The Zarpa &mdash; story</p></body></html>"),
        ]);
        assert_eq!(
            document_match(&epub, DocumentKind::Epub, "zarpa")
                .unwrap()
                .as_deref(),
            Some("The Zarpa — story")
        );

        let fodt = "<?xml version=\"1.0\"?><office:document><office:body><text:p>Flat Zarpa</text:p></office:body></office:document>";
        assert_eq!(
            document_match(fodt.as_bytes(), DocumentKind::FlatOpenDocument, "zarpa")
                .unwrap()
                .as_deref(),
            Some("Flat Zarpa")
        );
    }

    #[test]
    fn searches_spreadsheet_text_and_numbers() {
        let xlsx = zip_bytes(&[
            (
                "[Content_Types].xml",
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Name</t></is></c><c r="B1" t="inlineStr"><is><t>Total</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Zarpa API</t></is></c><c r="B2"><v>4217.5</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        assert_eq!(
            document_match(&xlsx, DocumentKind::Spreadsheet, "zarpa")
                .unwrap()
                .as_deref(),
            Some("Zarpa API | 4217.5")
        );
        assert_eq!(
            document_match(&xlsx, DocumentKind::Spreadsheet, "4217")
                .unwrap()
                .as_deref(),
            Some("Zarpa API | 4217.5")
        );
        assert!(document_match(b"plain", DocumentKind::Spreadsheet, "x").is_err());
    }
}
