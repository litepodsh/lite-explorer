//! Outlook message (`.msg`) preview: a Compound File Binary (OLE2) container.
//! Known MAPI string properties are extracted and decoded. Read-only.

use std::io::Cursor;

use cfb::CompoundFile;
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct MsgPreview {
    pub subject: String,
    pub sender: String,
    pub to: String,
    pub cc: String,
    pub body: String,
    pub attachments: Vec<String>,
}

pub(crate) fn is_msg_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("msg")
}

#[tauri::command]
pub async fn open_msg(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<MsgPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_msg(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

/// Decodes a MAPI string stream: `...001F` is UTF-16LE, `...001E` is codepage text.
fn decode(value: &[u8], unicode: bool) -> String {
    if unicode {
        let units: Vec<u16> = (0..value.len() / 2)
            .map(|index| u16::from_le_bytes([value[index * 2], value[index * 2 + 1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    } else {
        String::from_utf8_lossy(value)
            .trim_end_matches('\0')
            .trim()
            .to_string()
    }
}

fn property_id(name: &str) -> Option<&str> {
    let name = name.rsplit(['\\', '/']).next().unwrap_or(name);
    let name = name.strip_prefix("__substg1.0_")?;
    if name.len() < 8 {
        return None;
    }
    Some(&name[..4])
}

pub(crate) fn parse_msg(bytes: &[u8]) -> Result<MsgPreview, String> {
    let mut compound =
        CompoundFile::open(Cursor::new(bytes)).map_err(|_| "Not an Outlook message")?;
    let mut preview = MsgPreview::default();
    let mut attachment_count = 0usize;

    let mut paths = Vec::new();
    for entry in compound.walk() {
        if entry.is_stream() {
            paths.push(entry.path().to_string_lossy().into_owned());
        }
    }
    paths.sort();

    for path in &paths {
        let Some(id) = property_id(path) else {
            continue;
        };
        let unicode = path.ends_with("001F");
        let ansi = path.ends_with("001E");
        if !unicode && !ansi {
            continue;
        }
        let mut value = Vec::new();
        if let Ok(mut stream) = compound.open_stream(path) {
            use std::io::Read;
            let _ = stream.read_to_end(&mut value);
        }
        let text = decode(&value, unicode);
        if text.is_empty() {
            continue;
        }
        match id {
            "0037" if preview.subject.is_empty() => preview.subject = text,
            "1000" if preview.body.is_empty() => preview.body = text,
            "0C1F" if preview.sender.is_empty() => preview.sender = text,
            "0E04" if preview.to.is_empty() => preview.to = text,
            "0E03" if preview.cc.is_empty() => preview.cc = text,
            "3704" if preview.subject.is_empty() => preview.subject = text,
            _ => {}
        }
    }

    // Attachments live under `__attach...` storage; count their display names.
    for path in &paths {
        if path.contains("__attach") && path.ends_with("3707001F") {
            attachment_count += 1;
        }
    }
    if attachment_count > 0 {
        preview.attachments = (1..=attachment_count)
            .map(|index| format!("Adjunto {index}"))
            .collect();
    }

    if preview.subject.is_empty() && preview.body.is_empty() && preview.sender.is_empty() {
        return Err("No readable message properties".to_string());
    }
    if preview.subject.is_empty() {
        preview.subject = "(sin asunto)".to_string();
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_cfb() {
        assert!(parse_msg(b"not an ole file").is_err());
    }

    #[test]
    fn decodes_property_ids() {
        assert_eq!(property_id("__substg1.0_0037001F"), Some("0037"));
        assert_eq!(
            property_id("Root Entry\\__substg1.0_1000001F"),
            Some("1000")
        );
        assert_eq!(property_id("junk"), None);
    }

    #[test]
    fn decodes_unicode_strings() {
        let raw = [0x48, 0x00, 0x69, 0x00, 0x00, 0x00];
        assert_eq!(decode(&raw, true), "Hi");
    }
}
