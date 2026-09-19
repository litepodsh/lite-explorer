//! Packet capture (`.pcap`) preview: reads the global header and lists packets
//! with a guessed protocol. Read-only; `.pcapng` isn't supported.

use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Most packets summarized from one capture.
const MAX_PACKETS: usize = 2000;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PcapPacket {
    pub index: usize,
    pub time_ms: i64,
    pub length: u32,
    pub protocol: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct PcapPreview {
    pub link_type: String,
    pub packets: Vec<PcapPacket>,
    pub truncated: bool,
}

pub(crate) fn is_pcap_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("pcap")
}

#[tauri::command]
pub async fn open_pcap(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<PcapPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_pcap(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn link_type_name(value: u32) -> String {
    match value {
        0 => "Null",
        1 => "Ethernet",
        101 => "Raw IP",
        105 => "IEEE 802.11",
        113 => "Linux SLL",
        127 => "Radiotap",
        276 => "Linux SLL2",
        _ => return format!("Tipo {value}"),
    }
    .to_string()
}

fn protocol_of(data: &[u8]) -> String {
    if data.len() < 14 {
        return String::new();
    }
    let ethertype = u16::from_be_bytes([data[12], data[13]]);
    match ethertype {
        0x0806 => "ARP".to_string(),
        0x86DD => "IPv6".to_string(),
        0x8100 => "VLAN".to_string(),
        0x0800 => {
            if data.len() < 14 + 20 {
                return "IPv4".to_string();
            }
            match data[14 + 9] {
                1 => "ICMP".to_string(),
                6 => "TCP".to_string(),
                17 => "UDP".to_string(),
                other => format!("IPv4/{other}"),
            }
        }
        other => format!("0x{other:04X}"),
    }
}

pub(crate) fn parse_pcap(bytes: &[u8]) -> Result<PcapPreview, String> {
    if bytes.len() < 24 {
        return Err("Not a pcap file".to_string());
    }
    let magic_le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let magic_be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let little = match (magic_le, magic_be) {
        (0xA1B2C3D4, _) | (0xA1B23C4D, _) => true,
        (_, 0xA1B2C3D4) | (_, 0xA1B23C4D) => false,
        _ => return Err("Not a pcap file".to_string()),
    };

    let u32_at = |offset: usize| -> u32 {
        let raw = [
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ];
        if little {
            u32::from_le_bytes(raw)
        } else {
            u32::from_be_bytes(raw)
        }
    };

    let link_type = u32_at(20);
    let mut preview = PcapPreview {
        link_type: link_type_name(link_type),
        ..Default::default()
    };

    let mut offset = 24;
    while offset + 16 <= bytes.len() {
        let seconds = u32_at(offset);
        let micros = u32_at(offset + 4);
        let included = u32_at(offset + 8) as usize;
        offset += 16;
        if offset + included > bytes.len() {
            break;
        }
        let data = &bytes[offset..offset + included];
        offset += included;
        if preview.packets.len() >= MAX_PACKETS {
            preview.truncated = true;
            break;
        }
        preview.packets.push(PcapPacket {
            index: preview.packets.len() + 1,
            time_ms: seconds as i64 * 1000 + micros as i64 / 1000,
            length: included as u32,
            protocol: if link_type == 1 {
                protocol_of(data)
            } else {
                String::new()
            },
        });
    }

    if preview.packets.is_empty() {
        return Err("No packets found".to_string());
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(little: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        let put = |bytes: &mut Vec<u8>, value: u32| {
            let raw = if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            bytes.extend_from_slice(&raw);
        };
        put(&mut bytes, 0xA1B2C3D4);
        put(&mut bytes, 0x0002_0004);
        put(&mut bytes, 0);
        put(&mut bytes, 0);
        put(&mut bytes, 65535);
        put(&mut bytes, 1);
        bytes
    }

    fn packet(little: bool, frame: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let put = |bytes: &mut Vec<u8>, value: u32| {
            let raw = if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            bytes.extend_from_slice(&raw);
        };
        put(&mut bytes, 1_700_000_000);
        put(&mut bytes, 500_000);
        put(&mut bytes, frame.len() as u32);
        put(&mut bytes, frame.len() as u32);
        bytes.extend_from_slice(frame);
        bytes
    }

    #[test]
    fn parses_packets_and_protocol() {
        let mut frame = vec![0u8; 14];
        frame[12] = 0x08;
        frame[13] = 0x00;
        frame.extend_from_slice(&[0u8; 20]);
        frame[14 + 9] = 6;
        let mut bytes = header(true);
        bytes.extend_from_slice(&packet(true, &frame));
        let preview = parse_pcap(&bytes).unwrap();
        assert_eq!(preview.link_type, "Ethernet");
        assert_eq!(preview.packets.len(), 1);
        assert_eq!(preview.packets[0].protocol, "TCP");
        assert_eq!(preview.packets[0].time_ms, 1_700_000_000_500);
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse_pcap(b"not a capture at all").is_err());
    }
}
