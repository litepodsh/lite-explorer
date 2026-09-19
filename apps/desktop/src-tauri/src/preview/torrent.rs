//! BitTorrent metainfo (`.torrent`) preview: a small bencode decoder plus the
//! handful of fields worth showing. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentFile {
    pub path: String,
    pub size: i64,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TorrentPreview {
    pub name: String,
    pub announce: String,
    pub announce_list: Vec<String>,
    pub comment: String,
    pub created_by: String,
    pub creation_date: Option<i64>,
    pub piece_length: i64,
    pub total_size: i64,
    pub piece_count: usize,
    pub private: bool,
    pub files: Vec<TorrentFile>,
}

pub(crate) fn is_torrent_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("torrent")
}

#[tauri::command]
pub async fn open_torrent(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<TorrentPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_torrent(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

#[derive(Debug, Clone)]
enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Dict(Vec<(Vec<u8>, Value)>),
}

impl Value {
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    fn text(&self) -> Option<String> {
        self.bytes()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    }

    fn int(&self) -> Option<i64> {
        match self {
            Value::Int(value) => Some(*value),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(entries) => entries
                .iter()
                .find(|(name, _)| name == key.as_bytes())
                .map(|(_, value)| value),
            _ => None,
        }
    }
}

fn parse_value(bytes: &[u8], index: &mut usize) -> Result<Value, String> {
    let byte = *bytes.get(*index).ok_or("Unexpected end of bencode")?;
    match byte {
        b'i' => {
            *index += 1;
            let start = *index;
            while bytes.get(*index) != Some(&b'e') {
                *index += 1;
                if *index >= bytes.len() {
                    return Err("Unterminated integer".to_string());
                }
            }
            let text = std::str::from_utf8(&bytes[start..*index]).map_err(|_| "Bad integer")?;
            let value = text.parse::<i64>().map_err(|_| "Bad integer".to_string())?;
            *index += 1;
            Ok(Value::Int(value))
        }
        b'l' => {
            *index += 1;
            let mut items = Vec::new();
            while bytes.get(*index) != Some(&b'e') {
                items.push(parse_value(bytes, index)?);
            }
            *index += 1;
            Ok(Value::List(items))
        }
        b'd' => {
            *index += 1;
            let mut entries = Vec::new();
            while bytes.get(*index) != Some(&b'e') {
                let key = parse_value(bytes, index)?.text().ok_or("Non-string key")?;
                let value = parse_value(bytes, index)?;
                entries.push((key.into_bytes(), value));
            }
            *index += 1;
            Ok(Value::Dict(entries))
        }
        b'0'..=b'9' => {
            let start = *index;
            while bytes.get(*index).is_some_and(u8::is_ascii_digit) {
                *index += 1;
            }
            let length = std::str::from_utf8(&bytes[start..*index])
                .map_err(|_| "Bad length")?
                .parse::<usize>()
                .map_err(|_| "Bad length".to_string())?;
            if bytes.get(*index) != Some(&b':') {
                return Err("Missing colon".to_string());
            }
            *index += 1;
            let end = (*index).checked_add(length).ok_or("Length overflow")?;
            let slice = bytes.get(*index..end).ok_or("Truncated string")?;
            *index = end;
            Ok(Value::Bytes(slice.to_vec()))
        }
        _ => Err("Invalid bencode marker".to_string()),
    }
}

pub(crate) fn parse_torrent(bytes: &[u8]) -> Result<TorrentPreview, String> {
    let mut index = 0;
    let root = parse_value(bytes, &mut index)?;
    let info = root.get("info").ok_or("Missing info dictionary")?;

    let mut preview = TorrentPreview {
        name: info.get("name").and_then(Value::text).unwrap_or_default(),
        announce: root
            .get("announce")
            .and_then(Value::text)
            .unwrap_or_default(),
        comment: root
            .get("comment")
            .and_then(Value::text)
            .unwrap_or_default(),
        created_by: root
            .get("created by")
            .and_then(Value::text)
            .unwrap_or_default(),
        creation_date: root.get("creation date").and_then(Value::int),
        piece_length: info.get("piece length").and_then(Value::int).unwrap_or(0),
        piece_count: info
            .get("pieces")
            .and_then(Value::bytes)
            .map(|pieces| pieces.len() / 20)
            .unwrap_or(0),
        private: info.get("private").and_then(Value::int).unwrap_or(0) == 1,
        ..Default::default()
    };

    if let Some(Value::List(tiers)) = root.get("announce-list") {
        for tier in tiers {
            if let Value::List(urls) = tier {
                for url in urls {
                    if let Some(text) = url.text() {
                        preview.announce_list.push(text);
                    }
                }
            }
        }
    }

    if let Some(length) = info.get("length").and_then(Value::int) {
        preview.total_size = length;
        preview.files.push(TorrentFile {
            path: preview.name.clone(),
            size: length,
        });
    }
    if let Some(Value::List(files)) = info.get("files") {
        for file in files {
            let size = file.get("length").and_then(Value::int).unwrap_or(0);
            let path = file
                .get("path")
                .and_then(|value| match value {
                    Value::List(parts) => Some(
                        parts
                            .iter()
                            .filter_map(Value::text)
                            .collect::<Vec<_>>()
                            .join("/"),
                    ),
                    _ => None,
                })
                .unwrap_or_default();
            preview.total_size += size;
            preview.files.push(TorrentFile { path, size });
        }
    }

    if preview.name.is_empty() {
        return Err("Not a torrent file".to_string());
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn torrent() -> Vec<u8> {
        let piece = vec![0u8; 20];
        let mut info = b"d6:lengthi1000e4:name4:demo12:piece lengthi16384e6:pieces20:".to_vec();
        info.extend_from_slice(&piece);
        info.extend_from_slice(b"e");
        let announce = "https://tracker.example/a";
        let mut out = format!("d8:announce{}:{}4:info", announce.len(), announce).into_bytes();
        out.extend_from_slice(&info);
        out.extend_from_slice(b"e");
        out
    }

    #[test]
    fn parses_single_file_torrent() {
        let preview = parse_torrent(&torrent()).unwrap();
        assert_eq!(preview.name, "demo");
        assert_eq!(preview.announce, "https://tracker.example/a");
        assert_eq!(preview.total_size, 1000);
        assert_eq!(preview.piece_count, 1);
        assert_eq!(preview.files.len(), 1);
        assert_eq!(preview.files[0].path, "demo");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_torrent(b"not bencode").is_err());
    }
}
