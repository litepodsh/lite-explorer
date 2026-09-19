//! Disc image (`.iso`) preview: a minimal ISO 9660 reader that lists the files
//! and folders in the image, preferring Joliet names when present. Read-only.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

const SECTOR: usize = 2048;
/// Most entries listed from one image.
const MAX_ENTRIES: usize = 5000;
/// Directory recursion guard.
const MAX_DEPTH: usize = 32;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct IsoEntry {
    pub path: String,
    pub size: u32,
    pub is_dir: bool,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct IsoPreview {
    pub volume_name: String,
    pub entries: Vec<IsoEntry>,
    pub truncated: bool,
}

pub(crate) fn is_iso_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("iso")
}

#[tauri::command]
pub async fn open_iso(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<IsoPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_iso(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn decode_iso_name(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    match text.split_once(';') {
        Some((name, _)) => name.to_string(),
        None => text.into_owned(),
    }
}

fn decode_joliet_name(raw: &[u8]) -> String {
    let units: Vec<u16> = (0..raw.len() / 2)
        .map(|index| u16::from_be_bytes([raw[index * 2], raw[index * 2 + 1]]))
        .collect();
    String::from_utf16_lossy(&units)
        .split_once(';')
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| String::from_utf16_lossy(&units))
}

/// `true` for the `.` and `..` directory records.
fn is_dot(name: &[u8]) -> bool {
    name.len() == 1 && (name[0] == 0 || name[0] == 1)
}

struct Record {
    len: usize,
    extent: usize,
    size: u32,
    is_dir: bool,
    name: String,
}

fn read_record(bytes: &[u8], offset: usize, joliet: bool) -> Option<Record> {
    let length = *bytes.get(offset)? as usize;
    if length == 0 {
        return None;
    }
    let record = bytes.get(offset..offset + length)?;
    let extent = le_u32(record, 2) as usize;
    let size = le_u32(record, 10);
    let flags = *record.get(25)?;
    let name_len = *record.get(32)? as usize;
    let raw_name = record.get(33..33 + name_len)?;
    if is_dot(raw_name) {
        return Some(Record {
            len: length,
            extent,
            size,
            is_dir: true,
            name: if raw_name[0] == 0 {
                ".".into()
            } else {
                "..".into()
            },
        });
    }
    let name = if joliet {
        decode_joliet_name(raw_name)
    } else {
        decode_iso_name(raw_name)
    };
    Some(Record {
        len: length,
        extent,
        size,
        is_dir: flags & 0x02 != 0,
        name,
    })
}

fn walk(
    bytes: &[u8],
    extent: usize,
    size: u32,
    prefix: &str,
    joliet: bool,
    entries: &mut Vec<IsoEntry>,
    truncated: &mut bool,
) {
    let depth = if prefix.is_empty() {
        0
    } else {
        prefix.matches('/').count() + 1
    };
    if depth > MAX_DEPTH || *truncated {
        return;
    }
    let start = extent * SECTOR;
    let end = (start + size as usize).min(bytes.len());
    let mut offset = start;
    while offset < end {
        match read_record(bytes, offset, joliet) {
            None => {
                // Records never straddle a sector; jump to the next one.
                offset = ((offset / SECTOR) + 1) * SECTOR;
                continue;
            }
            Some(record) => {
                offset += record.len;
                if record.name == "." || record.name == ".." {
                    continue;
                }
                if entries.len() >= MAX_ENTRIES {
                    *truncated = true;
                    return;
                }
                let path = if prefix.is_empty() {
                    record.name.clone()
                } else {
                    format!("{prefix}/{}", record.name)
                };
                entries.push(IsoEntry {
                    path: path.clone(),
                    size: record.size,
                    is_dir: record.is_dir,
                });
                if record.is_dir {
                    walk(
                        bytes,
                        record.extent,
                        record.size,
                        &path,
                        joliet,
                        entries,
                        truncated,
                    );
                }
            }
        }
    }
}

pub(crate) fn parse_iso(bytes: &[u8]) -> Result<IsoPreview, String> {
    let pvd = 16 * SECTOR;
    if bytes.len() < pvd + SECTOR || &bytes[pvd + 1..pvd + 6] != b"CD001" {
        return Err("Not an ISO 9660 image".to_string());
    }

    // Look for a Joliet supplementary descriptor to get UTF-16 names.
    let mut joliet = false;
    let mut descriptor = pvd;
    while descriptor + SECTOR <= bytes.len() {
        let kind = bytes[descriptor];
        if kind == 255 {
            break;
        }
        if kind == 2 {
            let escape = &bytes[descriptor + 88..descriptor + 91];
            if escape.starts_with(&[0x25, 0x2F]) {
                joliet = true;
                break;
            }
        }
        descriptor += SECTOR;
    }
    let root_offset = (if joliet { descriptor } else { pvd }) + 156;
    let root = read_record(bytes, root_offset, joliet).ok_or("Missing root directory")?;

    let volume_name = {
        let raw = &bytes[(if joliet { descriptor } else { pvd }) + 40..(if joliet {
            descriptor
        } else {
            pvd
        }) + 72];
        let name = if joliet {
            decode_joliet_name(raw)
        } else {
            decode_iso_name(raw)
        };
        name.trim_matches('\0').trim().to_string()
    };

    let mut entries = Vec::new();
    let mut truncated = false;
    walk(
        bytes,
        root.extent,
        root.size,
        "",
        joliet,
        &mut entries,
        &mut truncated,
    );

    if entries.is_empty() {
        return Err("Empty disc image".to_string());
    }
    Ok(IsoPreview {
        volume_name,
        entries,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_iso() {
        assert!(parse_iso(b"not an iso").is_err());
    }

    #[test]
    fn decodes_iso_names() {
        assert_eq!(decode_iso_name(b"LEEME.TXT;1"), "LEEME.TXT");
        assert_eq!(decode_joliet_name(&[0x00, 0x4C, 0x00, 0x45]), "LE");
    }
}
