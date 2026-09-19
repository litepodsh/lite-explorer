//! vCard (`.vcf`) preview: parses one or more contacts out of a file. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{decode_text, read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub name: String,
    pub organization: String,
    pub title: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub addresses: Vec<String>,
    pub urls: Vec<String>,
    pub note: String,
}

pub(crate) fn is_vcard_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "vcf" | "vcard")
}

#[tauri::command]
pub async fn open_vcards(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<Vec<Contact>, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || {
        let text = decode_text(&bytes)?;
        parse_vcards(&text)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Joins folded lines (continuations start with a space or tab) into logical lines.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with([' ', '\t']) {
            if let Some(last) = lines.last_mut() {
                last.push_str(&line[1..]);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines
}

fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(',') => out.push(','),
            Some(';') => out.push(';'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

fn split_property(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let (head, value) = line.split_at(colon);
    let key = head
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_uppercase();
    Some((key, unescape(value[1..].trim())))
}

fn format_name(n: &str) -> String {
    let parts: Vec<&str> = n.split(';').collect();
    if parts.len() < 2 {
        return n.trim().to_string();
    }
    let given = parts[1].trim();
    let family = parts[0].trim();
    match (given.is_empty(), family.is_empty()) {
        (false, false) => format!("{given} {family}"),
        (false, true) => given.to_string(),
        _ => family.to_string(),
    }
}

fn format_address(value: &str) -> String {
    value
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn parse_vcards(text: &str) -> Result<Vec<Contact>, String> {
    let mut contacts = Vec::new();
    let mut current: Option<Contact> = None;
    let mut name: Option<String> = None;

    for line in unfold(text) {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("BEGIN:VCARD") {
            current = Some(Contact::default());
            name = None;
            continue;
        }
        if trimmed.eq_ignore_ascii_case("END:VCARD") {
            if let Some(mut contact) = current.take() {
                if contact.name.is_empty() {
                    contact.name = name.take().unwrap_or_default();
                }
                if contact.name.is_empty() {
                    contact.name = contact
                        .organization
                        .clone()
                        .split_whitespace()
                        .next()
                        .map(str::to_string)
                        .unwrap_or_else(|| "(sin nombre)".to_string());
                }
                contacts.push(contact);
            }
            continue;
        }
        let Some(contact) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = split_property(trimmed) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "FN" => contact.name = value,
            "N" => name = Some(format_name(&value)),
            "ORG" => contact.organization = value.replace(';', " ").trim().to_string(),
            "TITLE" => contact.title = value,
            "EMAIL" => contact.emails.push(value),
            "TEL" => contact.phones.push(value),
            "ADR" => {
                let address = format_address(&value);
                if !address.is_empty() {
                    contact.addresses.push(address);
                }
            }
            "URL" => contact.urls.push(value),
            "NOTE" => contact.note = value,
            _ => {}
        }
    }

    if contacts.is_empty() {
        return Err("No contacts found".to_string());
    }
    Ok(contacts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCARD\r\nVERSION:3.0\r\nN:García;Ana;;;\r\nFN:Ana García\r\nORG:Lite Explorer\r\nTITLE:Ingeniera\r\nEMAIL;TYPE=WORK:ana@lite.dev\r\nTEL;TYPE=CELL:+57 300 555 0101\r\nADR;TYPE=WORK:;;Calle 93 #13-45;Bogotá;;110221;Colombia\r\nURL:https://lite.dev\r\nNOTE:Línea uno\\nLínea dos\r\nEND:VCARD\r\n";

    #[test]
    fn parses_a_single_contact() {
        let contacts = parse_vcards(SAMPLE).unwrap();
        assert_eq!(contacts.len(), 1);
        let contact = &contacts[0];
        assert_eq!(contact.name, "Ana García");
        assert_eq!(contact.organization, "Lite Explorer");
        assert_eq!(contact.emails, vec!["ana@lite.dev"]);
        assert_eq!(contact.phones, vec!["+57 300 555 0101"]);
        assert_eq!(
            contact.addresses,
            vec!["Calle 93 #13-45, Bogotá, 110221, Colombia"]
        );
        assert_eq!(contact.note, "Línea uno\nLínea dos");
    }

    #[test]
    fn parses_multiple_contacts_and_folds() {
        let text = "BEGIN:VCARD\nFN:Luis\nNOTE:hola\n  mundo\nEND:VCARD\nBEGIN:VCARD\nN:Pérez;Camila;;;\nEND:VCARD\n";
        let contacts = parse_vcards(text).unwrap();
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].name, "Luis");
        assert_eq!(contacts[0].note, "hola mundo");
        assert_eq!(contacts[1].name, "Camila Pérez");
    }

    #[test]
    fn errors_without_cards() {
        assert!(parse_vcards("not a vcard").is_err());
    }
}
