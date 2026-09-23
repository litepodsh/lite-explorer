//! Document properties and statistics from OOXML and OpenDocument packages.
use super::{clip, format_date, group, read_zip_text, DocKind, InfoSection, Zip};
use crate::preview::xml::{self, Element};
use std::fs::File;

const SHEET_NAMES_MAX: usize = 10;

fn part(zip: &mut Zip, name: &str) -> Element {
    read_zip_text(zip, name)
        .map(|text| xml::parse_document(&text))
        .unwrap_or_default()
}

fn text_of(root: &Element, local: &str) -> Option<String> {
    root.descendant(local)
        .map(|element| clip(&element.text_content()))
        .filter(|value| !value.is_empty())
}

fn count_of(root: &Element, local: &str) -> Option<String> {
    text_of(root, local)?.parse::<u64>().ok().map(group)
}

fn sheets(section: &mut InfoSection, names: Vec<String>) {
    if names.is_empty() {
        return;
    }
    section.push("Sheets", group(names.len() as u64));
    if names.len() <= SHEET_NAMES_MAX {
        section.push("Sheet names", clip(&names.join(", ")));
    }
}

pub(crate) fn ooxml_section(zip: &mut Zip, kind: DocKind) -> Option<InfoSection> {
    let core = part(zip, "docProps/core.xml");
    let app = part(zip, "docProps/app.xml");
    let mut section = InfoSection::new("Document");
    section.push_opt("Title", text_of(&core, "title"));
    section.push_opt("Subject", text_of(&core, "subject"));
    section.push_opt("Author", text_of(&core, "creator"));
    section.push_opt("Last modified by", text_of(&core, "lastModifiedBy"));
    section.push_opt(
        "Created",
        text_of(&core, "created").map(|value| format_date(&value)),
    );
    section.push_opt(
        "Modified",
        text_of(&core, "modified").map(|value| format_date(&value)),
    );
    section.push_opt("Revision", text_of(&core, "revision"));
    match kind {
        DocKind::Text => {
            section.push_opt("Pages", count_of(&app, "Pages"));
            section.push_opt("Words", count_of(&app, "Words"));
            section.push_opt("Characters", count_of(&app, "Characters"));
        }
        DocKind::Spreadsheet => {
            let workbook = part(zip, "xl/workbook.xml");
            sheets(
                &mut section,
                workbook
                    .descendants_named("sheet")
                    .filter_map(|sheet| sheet.attr("name").map(str::to_string))
                    .collect(),
            );
        }
        DocKind::Presentation => {
            section.push_opt("Slides", count_of(&app, "Slides"));
            section.push_opt("Notes", count_of(&app, "Notes"));
            section.push_opt("Hidden slides", count_of(&app, "HiddenSlides"));
        }
        DocKind::Other => {}
    }
    let application: Vec<String> = [text_of(&app, "Application"), text_of(&app, "AppVersion")]
        .into_iter()
        .flatten()
        .collect();
    section.push("Application", application.join(" "));
    section.non_empty()
}

pub(crate) fn odf_section(zip: &mut Zip, kind: DocKind) -> Option<InfoSection> {
    let meta = part(zip, "meta.xml");
    let mut section = InfoSection::new("Document");
    section.push_opt("Title", text_of(&meta, "title"));
    section.push_opt("Subject", text_of(&meta, "subject"));
    section.push_opt("Author", text_of(&meta, "initial-creator"));
    section.push_opt("Last modified by", text_of(&meta, "creator"));
    section.push_opt(
        "Created",
        text_of(&meta, "creation-date").map(|value| format_date(&value)),
    );
    section.push_opt(
        "Modified",
        text_of(&meta, "date").map(|value| format_date(&value)),
    );
    section.push_opt("Revision", text_of(&meta, "editing-cycles"));
    let statistics = meta.descendant("document-statistic");
    let statistic = |name: &str| {
        statistics
            .and_then(|element| element.attr(name))
            .and_then(|value| value.parse::<u64>().ok())
            .map(group)
    };
    match kind {
        DocKind::Text => {
            section.push_opt("Pages", statistic("page-count"));
            section.push_opt("Words", statistic("word-count"));
            section.push_opt("Characters", statistic("character-count"));
        }
        DocKind::Spreadsheet => {
            let content = part(zip, "content.xml");
            sheets(
                &mut section,
                content
                    .descendants_named("table")
                    .filter_map(|table| table.attr("name").map(str::to_string))
                    .collect(),
            );
        }
        DocKind::Presentation => {
            let slides = part(zip, "content.xml").descendants_named("page").count();
            if slides > 0 {
                section.push("Slides", group(slides as u64));
            }
        }
        DocKind::Other => {}
    }
    section.push_opt("Application", text_of(&meta, "generator"));
    section.non_empty()
}

/// Password-protected OOXML is a compound file with an `EncryptionInfo` stream; nothing else is readable.
pub(crate) fn encrypted_section(file: File) -> Option<InfoSection> {
    let compound = cfb::CompoundFile::open(file).ok()?;
    if !compound.is_stream("/EncryptionInfo") {
        return None;
    }
    let mut section = InfoSection::new("Document");
    section.push("Protection", "Password protected");
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::super::{
        format_date,
        test_support::{rows, rows_for, zip_bytes},
    };
    use std::io::Write;

    const CONTENT_TYPES: (&str, &str) = ("[Content_Types].xml", "<Types/>");

    #[test]
    fn docx_properties_and_statistics() {
        let core = r#"<?xml version="1.0" encoding="UTF-8"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:title>R&amp;D Report</dc:title><dc:creator>Ana</dc:creator><cp:lastModifiedBy>Luis</cp:lastModifiedBy><cp:revision>4</cp:revision><dcterms:created>2024-03-01T14:22:00Z</dcterms:created></cp:coreProperties>"#;
        let app = "<Properties><Pages>12</Pages><Words>3456</Words><Characters>19876</Characters><Application>Microsoft Office Word</Application><AppVersion>16.0000</AppVersion></Properties>";
        let bytes = zip_bytes(&[
            CONTENT_TYPES,
            ("word/document.xml", "<w:document/>"),
            ("docProps/core.xml", core),
            ("docProps/app.xml", app),
        ]);
        let created = format_date("2024-03-01T14:22:00Z");
        assert_eq!(
            rows_for(&bytes, "report.docx", "Document"),
            rows(&[
                ("Title", "R&D Report"),
                ("Author", "Ana"),
                ("Last modified by", "Luis"),
                ("Created", &created),
                ("Revision", "4"),
                ("Pages", "12"),
                ("Words", "3,456"),
                ("Characters", "19,876"),
                ("Application", "Microsoft Office Word 16.0000"),
            ])
        );
    }

    #[test]
    fn xlsx_sheets_and_pptx_slides() {
        let workbook = r#"<workbook><sheets><sheet name="Ventas" sheetId="1"/><sheet name="Gastos" sheetId="2"/></sheets></workbook>"#;
        let xlsx = zip_bytes(&[CONTENT_TYPES, ("xl/workbook.xml", workbook)]);
        assert_eq!(
            rows_for(&xlsx, "book.xlsx", "Document"),
            rows(&[("Sheets", "2"), ("Sheet names", "Ventas, Gastos")])
        );
        let app = "<Properties><Slides>20</Slides><Notes>3</Notes><HiddenSlides>1</HiddenSlides></Properties>";
        let pptx = zip_bytes(&[
            CONTENT_TYPES,
            ("ppt/presentation.xml", "<p:presentation/>"),
            ("docProps/app.xml", app),
        ]);
        assert_eq!(
            rows_for(&pptx, "deck.pptx", "Document"),
            rows(&[("Slides", "20"), ("Notes", "3"), ("Hidden slides", "1")])
        );
    }

    #[test]
    fn odt_meta_and_statistics() {
        let meta = r#"<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/"><office:meta><dc:title>Tesis</dc:title><meta:initial-creator>Ana</meta:initial-creator><meta:generator>LibreOffice/7.6</meta:generator><meta:document-statistic meta:page-count="30" meta:word-count="12000" meta:character-count="70000"/></office:meta></office:document-meta>"#;
        let bytes = zip_bytes(&[
            ("mimetype", "application/vnd.oasis.opendocument.text"),
            ("meta.xml", meta),
        ]);
        assert_eq!(
            rows_for(&bytes, "tesis.odt", "Document"),
            rows(&[
                ("Title", "Tesis"),
                ("Author", "Ana"),
                ("Pages", "30"),
                ("Words", "12,000"),
                ("Characters", "70,000"),
                ("Application", "LibreOffice/7.6"),
            ])
        );
    }

    #[test]
    fn encrypted_office_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret.docx");
        let mut compound = cfb::create(&path).unwrap();
        compound
            .create_stream("/EncryptionInfo")
            .unwrap()
            .write_all(&[4, 0, 4, 0])
            .unwrap();
        compound.flush().unwrap();
        drop(compound);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            rows_for(&bytes, "secret.docx", "Document"),
            rows(&[("Protection", "Password protected")])
        );
    }

    #[test]
    fn truncated_zip_has_no_sections() {
        let mut bytes = zip_bytes(&[CONTENT_TYPES, ("word/document.xml", "<w:document/>")]);
        bytes.truncate(40);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.docx");
        std::fs::write(&path, &bytes).unwrap();
        assert!(super::super::sections(&path).is_empty());
    }
}
