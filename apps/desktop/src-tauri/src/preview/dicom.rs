//! DICOM image (`.dcm`) preview: a minimal parser for the file meta group and the
//! dataset, extracting patient/study metadata and the pixel data. Pixels are sent
//! to the frontend as base64 so it can window/level them on a canvas. Read-only.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::State;

use crate::app::db::Database;
use crate::preview::source::{read_bytes, SOURCE_MAX_BYTES};
use crate::{network, remote};

/// Largest pixel payload sent to the viewer.
const MAX_PIXELS: usize = 64 * 1024 * 1024;

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct DicomPreview {
    pub patient: String,
    pub patient_id: String,
    pub modality: String,
    pub study_date: String,
    pub study_description: String,
    pub rows: u32,
    pub columns: u32,
    pub bits_allocated: u32,
    pub photometric: String,
    pub window_center: Option<f64>,
    pub window_width: Option<f64>,
    pub frames: u32,
    /// Base64 of the raw pixel samples (8-bit unsigned, or 16-bit little-endian).
    pub pixels: String,
}

pub(crate) fn is_dicom_extension(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "dcm" | "dicom")
}

#[tauri::command]
pub async fn open_dicom(
    database: State<'_, Database>,
    clients: State<'_, remote::RemoteClients>,
    sessions: State<'_, network::servers::Sessions>,
    path: String,
) -> Result<DicomPreview, String> {
    let bytes = read_bytes(&database.0, &clients, &sessions, &path, SOURCE_MAX_BYTES).await?;
    tauri::async_runtime::spawn_blocking(move || parse_dicom(&bytes))
        .await
        .map_err(|error| error.to_string())?
}

#[derive(Clone, Copy, PartialEq)]
enum Endian {
    Little,
    Big,
}

impl Endian {
    fn u16(self, bytes: &[u8]) -> u16 {
        let pair = [bytes[0], bytes[1]];
        match self {
            Endian::Little => u16::from_le_bytes(pair),
            Endian::Big => u16::from_be_bytes(pair),
        }
    }

    fn u32(self, bytes: &[u8]) -> u32 {
        let quad = [bytes[0], bytes[1], bytes[2], bytes[3]];
        match self {
            Endian::Little => u32::from_le_bytes(quad),
            Endian::Big => u32::from_be_bytes(quad),
        }
    }
}

struct Element<'a> {
    group: u16,
    element: u16,
    value: &'a [u8],
}

/// Long-VR types carry a 2-byte reserved field and a 4-byte length.
fn is_long_vr(vr: &str) -> bool {
    matches!(
        vr,
        "OB" | "OW" | "OF" | "SQ" | "UT" | "UN" | "OD" | "OL" | "OV" | "SV" | "UV"
    )
}

fn read_element<'a>(
    bytes: &'a [u8],
    position: &mut usize,
    explicit: bool,
    endian: Endian,
) -> Option<Element<'a>> {
    if *position + 8 > bytes.len() {
        return None;
    }
    let group = endian.u16(&bytes[*position..]);
    let element = endian.u16(&bytes[*position + 2..]);
    *position += 4;
    let length = if explicit {
        let vr = String::from_utf8_lossy(&bytes[*position..*position + 2]).into_owned();
        *position += 2;
        if is_long_vr(&vr) {
            if *position + 6 > bytes.len() {
                return None;
            }
            let length = endian.u32(&bytes[*position + 2..]) as usize;
            *position += 6;
            length
        } else {
            if *position + 2 > bytes.len() {
                return None;
            }
            let length = endian.u16(&bytes[*position..]) as usize;
            *position += 2;
            length
        }
    } else {
        if *position + 4 > bytes.len() {
            return None;
        }
        let length = endian.u32(&bytes[*position..]) as usize;
        *position += 4;
        length
    };
    if length == 0xFFFF_FFFF || *position + length > bytes.len() {
        // Undefined-length sequences aren't supported; stop cleanly.
        return None;
    }
    let value = &bytes[*position..*position + length];
    *position += length;
    Some(Element {
        group,
        element,
        value,
    })
}

fn text(value: &[u8]) -> String {
    String::from_utf8_lossy(value)
        .trim_matches(|ch: char| ch == '\0' || ch == ' ')
        .to_string()
}

fn unsigned(value: &[u8], endian: Endian) -> Option<u32> {
    match value.len() {
        2 => Some(endian.u16(value) as u32),
        4 => Some(endian.u32(value)),
        _ => None,
    }
}

fn decimal(value: &[u8]) -> Option<f64> {
    text(value)
        .split('\\')
        .next()
        .and_then(|part| part.trim().parse().ok())
}

pub(crate) fn parse_dicom(bytes: &[u8]) -> Result<DicomPreview, String> {
    if bytes.len() < 132 || &bytes[128..132] != b"DICM" {
        return Err("Not a DICOM file".to_string());
    }

    // The meta group is always explicit VR little-endian; it names the transfer syntax.
    let mut position = 132;
    let mut transfer_syntax = "1.2.840.10008.1.2.1".to_string();
    while position + 8 <= bytes.len() {
        let start = position;
        let Some(element) = read_element(bytes, &mut position, true, Endian::Little) else {
            position = start;
            break;
        };
        if element.group != 0x0002 {
            position = start;
            break;
        }
        if element.element == 0x0010 {
            transfer_syntax = text(element.value);
        }
    }

    let explicit = transfer_syntax != "1.2.840.10008.1.2";
    let endian = if transfer_syntax == "1.2.840.10008.1.2.2" {
        Endian::Big
    } else {
        Endian::Little
    };

    let mut preview = DicomPreview {
        bits_allocated: 8,
        photometric: "MONOCHROME2".to_string(),
        frames: 1,
        ..Default::default()
    };
    let mut pixel_data: Option<Vec<u8>> = None;

    while position + 8 <= bytes.len() {
        let Some(element) = read_element(bytes, &mut position, explicit, endian) else {
            break;
        };
        match (element.group, element.element) {
            (0x0010, 0x0010) => preview.patient = text(element.value),
            (0x0010, 0x0020) => preview.patient_id = text(element.value),
            (0x0008, 0x0060) => preview.modality = text(element.value),
            (0x0008, 0x0020) => preview.study_date = text(element.value),
            (0x0008, 0x1030) => preview.study_description = text(element.value),
            (0x0028, 0x0010) => preview.rows = unsigned(element.value, endian).unwrap_or(0),
            (0x0028, 0x0011) => preview.columns = unsigned(element.value, endian).unwrap_or(0),
            (0x0028, 0x0002) => preview.frames = 1,
            (0x0028, 0x0004) => preview.photometric = text(element.value),
            (0x0028, 0x0100) => {
                preview.bits_allocated = unsigned(element.value, endian).unwrap_or(8)
            }
            (0x0028, 0x1050) => preview.window_center = decimal(element.value),
            (0x0028, 0x1051) => preview.window_width = decimal(element.value),
            (0x7FE0, 0x0010) => pixel_data = Some(element.value.to_vec()),
            _ => {}
        }
    }

    if preview.rows == 0 || preview.columns == 0 {
        return Err("Image has no pixel grid".to_string());
    }
    let Some(pixels) = pixel_data else {
        return Err("No pixel data found".to_string());
    };
    let pixels = if pixels.len() > MAX_PIXELS {
        return Err("Pixel data is too large to preview".to_string());
    } else {
        pixels
    };
    preview.pixels = STANDARD.encode(pixels);
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes.extend_from_slice(b"DICM");
        let element =
            |bytes: &mut Vec<u8>, group: u16, element: u16, vr: &[u8; 2], value: &[u8]| {
                bytes.extend_from_slice(&group.to_le_bytes());
                bytes.extend_from_slice(&element.to_le_bytes());
                bytes.extend_from_slice(vr);
                if is_long_vr(&String::from_utf8_lossy(vr)) {
                    bytes.extend_from_slice(&[0, 0]);
                    bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
                } else {
                    bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
                }
                bytes.extend_from_slice(value);
            };
        element(&mut bytes, 0x0002, 0x0010, b"UI", b"1.2.840.10008.1.2.1\0");
        element(&mut bytes, 0x0010, 0x0010, b"PN", b"GARCIA^ANA ");
        element(&mut bytes, 0x0008, 0x0060, b"CS", b"OT");
        element(&mut bytes, 0x0028, 0x0010, b"US", &2u16.to_le_bytes());
        element(&mut bytes, 0x0028, 0x0011, b"US", &2u16.to_le_bytes());
        element(&mut bytes, 0x0028, 0x0004, b"CS", b"MONOCHROME2 ");
        element(&mut bytes, 0x0028, 0x0100, b"US", &8u16.to_le_bytes());
        element(&mut bytes, 0x7FE0, 0x0010, b"OB", &[0, 64, 128, 255]);
        bytes
    }

    #[test]
    fn parses_metadata_and_pixels() {
        let preview = parse_dicom(&sample()).unwrap();
        assert_eq!(preview.patient, "GARCIA^ANA");
        assert_eq!(preview.modality, "OT");
        assert_eq!(preview.rows, 2);
        assert_eq!(preview.columns, 2);
        assert_eq!(preview.pixels, STANDARD.encode([0u8, 64, 128, 255]));
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse_dicom(b"not a dicom file").is_err());
    }
}
