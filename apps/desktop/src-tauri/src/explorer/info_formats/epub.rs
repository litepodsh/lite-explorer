//! Book metadata from an EPUB package document, found through `META-INF/container.xml`.
use super::{clip, format_date, group, read_zip_text, InfoSection, Zip};
use crate::preview::xml::{self, Element};

pub(crate) fn section(zip: &mut Zip) -> Option<InfoSection> {
    let container = xml::parse_document(&read_zip_text(zip, "META-INF/container.xml")?);
    let package_path = container
        .descendant("rootfile")?
        .attr("full-path")?
        .to_string();
    let document = xml::parse_document(&read_zip_text(zip, &package_path)?);
    let package = document.descendant("package")?;
    let metadata = package.descendant("metadata")?;
    let text = |local: &str| {
        metadata
            .descendant(local)
            .map(|element| clip(&element.text_content()))
            .filter(|value| !value.is_empty())
    };
    let mut section = InfoSection::new("Book");
    section.push_opt("Title", text("title"));
    let authors: Vec<String> = metadata
        .descendants_named("creator")
        .map(|element| element.text_content().trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    if !authors.is_empty() {
        section.push("Authors", clip(&authors.join(", ")));
    }
    section.push_opt("Publisher", text("publisher"));
    section.push_opt("Language", text("language"));
    section.push_opt("Published", text("date").map(|value| format_date(&value)));
    section.push_opt("Identifier", identifier(metadata));
    section.push_opt("EPUB version", package.attr("version").map(str::to_string));
    let chapters = package.descendants_named("itemref").count();
    if chapters > 0 {
        section.push("Chapters", group(chapters as u64));
    }
    section.non_empty()
}

/// The ISBN when one of the identifiers is an ISBN, otherwise the first identifier.
fn identifier(metadata: &Element) -> Option<String> {
    let identifiers: Vec<String> = metadata
        .descendants_named("identifier")
        .map(|element| element.text_content().trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    identifiers
        .iter()
        .find(|value| is_isbn(value))
        .or(identifiers.first())
        .map(|value| clip(value))
}

fn is_isbn(value: &str) -> bool {
    let characters: String = value
        .trim_start_matches("urn:isbn:")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    matches!(characters.len(), 10 | 13)
        && characters
            .chars()
            .all(|c| c.is_ascii_digit() || c == 'X' || c == 'x')
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{rows, rows_for, zip_bytes};

    #[test]
    fn epub_package_metadata() {
        let container = r#"<container><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
        let package = r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Cien años de soledad</dc:title><dc:creator>Gabriel García Márquez</dc:creator><dc:creator>Edith Grossman</dc:creator><dc:publisher>Harper</dc:publisher><dc:language>es</dc:language><dc:date>1967-05-30</dc:date><dc:identifier>urn:uuid:1234</dc:identifier><dc:identifier>urn:isbn:978-0-06-088328-7</dc:identifier></metadata><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>"#;
        let bytes = zip_bytes(&[
            ("mimetype", "application/epub+zip"),
            ("META-INF/container.xml", container),
            ("OEBPS/content.opf", package),
        ]);
        assert_eq!(
            rows_for(&bytes, "libro.epub", "Book"),
            rows(&[
                ("Title", "Cien años de soledad"),
                ("Authors", "Gabriel García Márquez, Edith Grossman"),
                ("Publisher", "Harper"),
                ("Language", "es"),
                ("Published", "1967-05-30"),
                ("Identifier", "urn:isbn:978-0-06-088328-7"),
                ("EPUB version", "3.0"),
                ("Chapters", "3"),
            ])
        );
        assert!(rows_for(&bytes, "libro.epub", "Archive").is_empty());
    }
}
