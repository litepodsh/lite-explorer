//! FictionBook (`.fb2`) preview: parses the XML into title/author and body
//! paragraphs. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::preview::xml::parse_document;
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Fb2Preview {
    pub title: String,
    pub author: String,
    pub annotation: String,
    pub paragraphs: Vec<String>,
}

pub(crate) fn is_fb2_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("fb2")
}

#[tauri::command]
pub async fn open_fb2(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<Fb2Preview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_fb2(&text)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn text_of(element: Option<&crate::preview::xml::Element>) -> String {
    element
        .map(|element| element.text_content().trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn parse_fb2(text: &str) -> Result<Fb2Preview, String> {
    let root = parse_document(text);
    let description = root.descendant("description");
    let title_info = description.and_then(|description| description.descendant("title-info"));

    let title = text_of(title_info.and_then(|info| info.descendant("book-title")));
    let author = title_info
        .and_then(|info| info.descendant("author"))
        .map(|author| {
            let first = text_of(author.descendant("first-name"));
            let last = text_of(author.descendant("last-name"));
            let nickname = text_of(author.descendant("nickname"));
            [first, last]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
                .if_empty_then(nickname)
        })
        .unwrap_or_default();
    let annotation = text_of(title_info.and_then(|info| info.descendant("annotation")));

    let body = root.descendant("body").ok_or("No book body found")?;
    let mut paragraphs = Vec::new();
    collect_paragraphs(body, &mut paragraphs);
    if paragraphs.is_empty() {
        return Err("No readable text found".to_string());
    }
    Ok(Fb2Preview {
        title: if title.is_empty() {
            "(sin título)".to_string()
        } else {
            title
        },
        author,
        annotation,
        paragraphs,
    })
}

fn collect_paragraphs(element: &crate::preview::xml::Element, out: &mut Vec<String>) {
    for child in &element.children {
        match child.local() {
            "title" => {
                let text = child.text_content().trim().to_string();
                if !text.is_empty() {
                    out.push(format!("# {text}"));
                }
            }
            "p" | "subtitle" => {
                let text = child.text_content().trim().to_string();
                if !text.is_empty() {
                    out.push(text);
                }
            }
            "section" | "body" | "epigraph" | "poem" | "stanza" | "cite" => {
                collect_paragraphs(child, out)
            }
            _ => {}
        }
    }
}

trait IfEmpty {
    fn if_empty_then(self, fallback: String) -> String;
}

impl IfEmpty for String {
    fn if_empty_then(self, fallback: String) -> String {
        if self.is_empty() {
            fallback
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">
<description><title-info><book-title>La Odisea</book-title>
<author><first-name>Homero</first-name></author>
<annotation>Un viaje épico.</annotation></title-info></description>
<body><section><title><p>Capítulo uno</p></title>
<p>Háblame, Musa.</p><p>Mucho anduvo errante.</p></section></body></FictionBook>"#;

    #[test]
    fn parses_title_author_and_paragraphs() {
        let preview = parse_fb2(SAMPLE).unwrap();
        assert_eq!(preview.title, "La Odisea");
        assert_eq!(preview.author, "Homero");
        assert_eq!(preview.annotation, "Un viaje épico.");
        assert_eq!(preview.paragraphs[0], "# Capítulo uno");
        assert_eq!(preview.paragraphs.len(), 3);
    }

    #[test]
    fn errors_without_body() {
        assert!(parse_fb2("<FictionBook><description/></FictionBook>").is_err());
    }
}
