//! Kindle e-book (`.mobi`, `.azw3`) preview: reads the PalmDOC/MOBI header and
//! decompresses the text records (uncompressed or PalmDOC LZ77) into paragraphs.
//! Read-only; DRM-protected and HUFF/CDIC books aren't supported.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most characters kept from one book.
const MAX_CHARS: usize = 400_000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MobiPreview {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub truncated: bool,
}

pub(crate) fn is_mobi_extension(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "mobi" | "azw3" | "azw"
    )
}

#[tauri::command]
pub async fn open_mobi(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<MobiPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_mobi(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
    ]))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset + 1)?,
        *bytes.get(offset + 2)?,
        *bytes.get(offset + 3)?,
    ]))
}

/// PalmDOC LZ77, the common Kindle text compression.
fn decompress_palmdoc(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut index = 0;
    while index < input.len() {
        let byte = input[index];
        index += 1;
        match byte {
            0x00 => out.push(0),
            0x01..=0x08 => {
                for _ in 0..byte {
                    if index < input.len() {
                        out.push(input[index]);
                        index += 1;
                    }
                }
            }
            0x09..=0x7F => out.push(byte),
            0x80..=0xBF => {
                let Some(next) = input.get(index).copied() else {
                    break;
                };
                index += 1;
                let pair = ((byte as u16) << 8) | next as u16;
                let distance = (pair >> 3) & 0x07FF;
                let length = (pair & 0x0007) + 3;
                if distance as usize > out.len() || distance == 0 {
                    break;
                }
                let start = out.len() - distance as usize;
                for step in 0..length {
                    let value = out[start + step as usize];
                    out.push(value);
                }
            }
            _ => {
                out.push(b' ');
                out.push(byte ^ 0x80);
            }
        }
    }
    out
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "mdash" => Some('—'),
        "ndash" => Some('–'),
        "hellip" => Some('…'),
        "ldquo" => Some('“'),
        "rdquo" => Some('”'),
        _ if entity.starts_with('#') => {
            let code = entity.trim_start_matches('#');
            let value = if let Some(hex) = code.strip_prefix(['x', 'X']) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                code.parse().ok()?
            };
            char::from_u32(value)
        }
        _ => None,
    }
}

/// Turns MOBI's HTML-ish markup into plain paragraphs.
fn html_to_paragraphs(html: &str) -> Vec<String> {
    let mut text = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                let mut tag = String::new();
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                    tag.push(next);
                }
                let name = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if matches!(
                    name,
                    "p" | "br" | "div" | "h1" | "h2" | "h3" | "mbp:pagebreak"
                ) {
                    text.push('\n');
                }
            }
            '&' => {
                let mut entity = String::new();
                for next in chars.by_ref() {
                    if next == ';' || entity.len() > 10 {
                        break;
                    }
                    entity.push(next);
                }
                text.push(decode_entity(&entity).unwrap_or(' '));
            }
            _ => text.push(ch),
        }
    }

    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect()
}

pub(crate) fn parse_mobi(bytes: &[u8]) -> Result<MobiPreview, String> {
    if bytes.len() < 78 {
        return Err("Not a Kindle book".to_string());
    }
    let record_count = be_u16(bytes, 76).ok_or("Truncated header")? as usize;
    if record_count == 0 {
        return Err("Not a Kindle book".to_string());
    }
    let record_offset = |index: usize| be_u32(bytes, 78 + index * 8).map(|value| value as usize);

    let first = record_offset(0).ok_or("Missing header record")?;
    let first_end = record_offset(1).ok_or("Missing text record")?;
    let header = bytes
        .get(first..first_end)
        .ok_or("Truncated header record")?;
    if header.len() < 16 {
        return Err("Truncated header record".to_string());
    }
    let compression = be_u16(header, 0).unwrap_or(0);
    let text_length = be_u32(header, 4).unwrap_or(0) as usize;
    let text_records = be_u16(header, 8).unwrap_or(0) as usize;
    let encryption = be_u16(header, 12).unwrap_or(0);
    if encryption != 0 {
        return Err("Book is DRM-protected".to_string());
    }
    if header.len() >= 20 && &header[16..20] != b"MOBI" {
        return Err("Not a MOBI book".to_string());
    }

    // Full title: a byte range inside record 0. The offset/length field pair has
    // moved between MOBI versions, so try the known layouts and keep the first
    // that points at a printable string.
    let mut title = String::new();
    for (offset_field, length_field) in [(68usize, 72usize), (72, 76)] {
        let Some(name_offset) = be_u32(header, 16 + offset_field).map(|value| value as usize)
        else {
            continue;
        };
        let Some(name_length) = be_u32(header, 16 + length_field).map(|value| value as usize)
        else {
            continue;
        };
        if name_length == 0 || name_length > 512 || name_offset + name_length > header.len() {
            continue;
        }
        let raw = &header[name_offset..name_offset + name_length];
        let candidate = String::from_utf8_lossy(raw).trim().to_string();
        if !candidate.is_empty() && !candidate.chars().any(char::is_control) {
            title = candidate;
            break;
        }
    }

    let mut raw_text = Vec::new();
    for index in 1..=text_records.min(record_count.saturating_sub(1)) {
        let Some(start) = record_offset(index) else {
            break;
        };
        let end = record_offset(index + 1)
            .unwrap_or(bytes.len())
            .min(bytes.len());
        let Some(chunk) = bytes.get(start..end) else {
            break;
        };
        match compression {
            1 => raw_text.extend_from_slice(chunk),
            2 => raw_text.extend_from_slice(&decompress_palmdoc(chunk)),
            _ => return Err(format!("Unsupported Kindle compression ({compression})")),
        }
        if raw_text.len() >= text_length && text_length > 0 {
            break;
        }
    }
    if text_length > 0 && raw_text.len() > text_length {
        raw_text.truncate(text_length);
    }

    let html = String::from_utf8_lossy(&raw_text);
    let mut paragraphs = html_to_paragraphs(&html);
    let mut truncated = false;
    let mut total = 0usize;
    paragraphs.retain(|paragraph| {
        total += paragraph.chars().count();
        if total > MAX_CHARS {
            truncated = true;
            return false;
        }
        true
    });

    if paragraphs.is_empty() {
        return Err("No readable text found".to_string());
    }
    Ok(MobiPreview {
        title: if title.is_empty() {
            "(sin título)".to_string()
        } else {
            title
        },
        paragraphs,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompresses_literals_and_backrefs() {
        // "abc" then a copy of the last 3 bytes -> "abcabc"
        let input = [0x61, 0x62, 0x63, 0x80, 0x18];
        assert_eq!(decompress_palmdoc(&input), b"abcabc");
    }

    #[test]
    fn strips_markup_into_paragraphs() {
        let paragraphs = html_to_paragraphs("<p>Hola&nbsp;mundo</p><p>Segundo &amp; tercero</p>");
        assert_eq!(paragraphs, vec!["Hola mundo", "Segundo & tercero"]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_mobi(b"not a kindle book").is_err());
    }
}
