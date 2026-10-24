use std::io::Cursor;

use img_parts::{Bytes, DynImage, ImageEXIF, ImageICC};

const ORIENTATION_TAG: u16 = 0x0112;

fn read_u16(data: &[u8], at: usize, little: bool) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(if little {
        u16::from_le_bytes([bytes[0], bytes[1]])
    } else {
        u16::from_be_bytes([bytes[0], bytes[1]])
    })
}

fn read_u32(data: &[u8], at: usize, little: bool) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    let array = [bytes[0], bytes[1], bytes[2], bytes[3]];
    Some(if little {
        u32::from_le_bytes(array)
    } else {
        u32::from_be_bytes(array)
    })
}

/// Sets the Orientation tag of IFD0 to 1. The pixels were already rotated while
/// converting, so a viewer must not rotate them again.
pub fn reset_orientation(exif: &mut [u8]) {
    let little = match exif.get(0..2) {
        Some([b'I', b'I']) => true,
        Some([b'M', b'M']) => false,
        _ => return,
    };
    let Some(ifd) = read_u32(exif, 4, little) else {
        return;
    };
    let ifd = ifd as usize;
    let Some(count) = read_u16(exif, ifd, little) else {
        return;
    };
    for index in 0..count as usize {
        let entry = ifd + 2 + index * 12;
        if read_u16(exif, entry, little) == Some(ORIENTATION_TAG) {
            let value = if little { 1u16.to_le_bytes() } else { 1u16.to_be_bytes() };
            if let Some(slot) = exif.get_mut(entry + 8..entry + 10) {
                slot.copy_from_slice(&value);
            }
            return;
        }
    }
}

/// EXIF (starting at its TIFF header) and ICC profile of a source image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
}

/// EXIF and ICC of a JPEG, PNG or WebP file. Other containers give an empty result.
pub fn read(source: &[u8]) -> Metadata {
    let Some(image) = DynImage::from_bytes(Bytes::copy_from_slice(source))
        .ok()
        .flatten()
    else {
        return Metadata::default();
    };
    Metadata {
        exif: image.exif().map(|exif| exif.to_vec()),
        icc: image.icc_profile().map(|icc| icc.to_vec()),
    }
}

/// Writes `metadata` into `output`, which must be a JPEG, PNG or WebP file; otherwise
/// `output` is returned untouched. EXIF orientation is reset because the pixels are
/// already upright.
pub fn apply(metadata: &Metadata, output: Vec<u8>) -> Vec<u8> {
    if metadata.exif.is_none() && metadata.icc.is_none() {
        return output;
    }
    let Some(mut to) = DynImage::from_bytes(Bytes::copy_from_slice(&output))
        .ok()
        .flatten()
    else {
        return output;
    };
    if let Some(exif) = &metadata.exif {
        let mut data = exif.clone();
        reset_orientation(&mut data);
        to.set_exif(Some(Bytes::from(data)));
    }
    if let Some(icc) = &metadata.icc {
        to.set_icc_profile(Some(Bytes::copy_from_slice(icc)));
    }
    let mut buffer = Vec::with_capacity(output.len() + 4096);
    match to.encoder().write_to(&mut buffer) {
        Ok(_) => buffer,
        Err(_) => output,
    }
}

/// Copies EXIF and ICC from `source` into `output` (both JPEG, PNG or WebP).
#[cfg(test)]
pub fn copy_metadata(source: &[u8], output: Vec<u8>) -> Vec<u8> {
    apply(&read(source), output)
}

/// Whether a TIFF-format EXIF block holds a GPS latitude.
pub fn exif_has_gps(exif: &[u8]) -> bool {
    exif::Reader::new()
        .read_raw(exif.to_vec())
        .ok()
        .is_some_and(|data| {
            data.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
                .is_some()
        })
}

pub fn has_gps(bytes: &[u8]) -> bool {
    exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .is_some_and(|data| {
            data.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY)
                .is_some()
        })
}

/// Little-endian TIFF block with a single IFD0 entry: Orientation.
#[cfg(test)]
pub(crate) fn exif_orientation_le(orientation: u16) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"II*\0");
    data.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
    data.extend_from_slice(&1u16.to_le_bytes()); // entry count
    data.extend_from_slice(&ORIENTATION_TAG.to_le_bytes());
    data.extend_from_slice(&3u16.to_le_bytes()); // SHORT
    data.extend_from_slice(&1u32.to_le_bytes()); // count
    data.extend_from_slice(&orientation.to_le_bytes());
    data.extend_from_slice(&[0, 0]); // padding
    data.extend_from_slice(&0u32.to_le_bytes()); // next IFD
    data
}

/// Little-endian TIFF block whose IFD0 points to a GPS IFD holding a latitude.
#[cfg(test)]
pub(crate) fn exif_with_gps() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(b"II*\0");
    data.extend_from_slice(&8u32.to_le_bytes());
    // IFD0 at 8: one entry, GPSInfo pointer (LONG) to the GPS IFD at 26.
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0x8825u16.to_le_bytes());
    data.extend_from_slice(&4u16.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&26u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    // GPS IFD at 26: GPSLatitude (RATIONAL x3) stored at offset 44.
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0x0002u16.to_le_bytes());
    data.extend_from_slice(&5u16.to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&44u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    for degrees in [1u32, 2, 3] {
        data.extend_from_slice(&degrees.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
    }
    data
}

#[cfg(test)]
pub(crate) fn with_exif(image: Vec<u8>, exif: &[u8]) -> Vec<u8> {
    let mut parsed = DynImage::from_bytes(Bytes::from(image))
        .unwrap()
        .expect("a JPEG, PNG or WebP image");
    parsed.set_exif(Some(Bytes::copy_from_slice(exif)));
    let mut out = Vec::new();
    parsed.encoder().write_to(&mut out).unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::convert::{encode::encode, formats::ImageFormat};
    use image::{DynamicImage, RgbaImage};

    fn jpeg() -> Vec<u8> {
        encode(
            &DynamicImage::ImageRgba8(RgbaImage::new(16, 16)),
            ImageFormat::Jpeg,
            80,
        )
        .unwrap()
    }

    fn orientation_of(bytes: &[u8]) -> Option<u32> {
        exif::Reader::new()
            .read_from_container(&mut Cursor::new(bytes))
            .ok()?
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
            .value
            .get_uint(0)
    }

    #[test]
    fn resets_little_endian_orientation() {
        let mut data = exif_orientation_le(6);
        reset_orientation(&mut data);
        assert_eq!(data, exif_orientation_le(1));
    }

    #[test]
    fn resets_big_endian_orientation() {
        let mut data = Vec::new();
        data.extend_from_slice(b"MM\0*");
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&ORIENTATION_TAG.to_be_bytes());
        data.extend_from_slice(&3u16.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&8u16.to_be_bytes());
        data.extend_from_slice(&[0, 0]);
        data.extend_from_slice(&0u32.to_be_bytes());
        reset_orientation(&mut data);
        assert_eq!(read_u16(&data, 18, false), Some(1));
    }

    #[test]
    fn malformed_exif_is_left_alone() {
        let mut garbage = b"not exif at all".to_vec();
        reset_orientation(&mut garbage);
        assert_eq!(garbage, b"not exif at all");
        let mut truncated = exif_orientation_le(6);
        truncated.truncate(12);
        let before = truncated.clone();
        reset_orientation(&mut truncated);
        assert_eq!(truncated, before);
    }

    #[test]
    fn copies_exif_and_resets_orientation() {
        let source = with_exif(jpeg(), &exif_orientation_le(6));
        assert_eq!(orientation_of(&source), Some(6));
        let result = copy_metadata(&source, jpeg());
        assert_eq!(orientation_of(&result), Some(1));
    }

    #[test]
    fn source_without_metadata_returns_output_unchanged() {
        let output = jpeg();
        assert_eq!(copy_metadata(&jpeg(), output.clone()), output);
    }

    #[test]
    fn unsupported_output_format_is_returned_unchanged() {
        let source = with_exif(jpeg(), &exif_orientation_le(6));
        let gif = encode(
            &DynamicImage::ImageRgba8(RgbaImage::new(4, 4)),
            ImageFormat::Gif,
            80,
        )
        .unwrap();
        assert_eq!(copy_metadata(&source, gif.clone()), gif);
    }

    #[test]
    fn metadata_read_then_apply_matches_copy() {
        let source = with_exif(jpeg(), &exif_orientation_le(6));
        let metadata = read(&source);
        assert_eq!(metadata.exif.as_deref(), Some(exif_orientation_le(6).as_slice()));
        assert_eq!(orientation_of(&apply(&metadata, jpeg())), Some(1));
        assert_eq!(read(b"not an image"), Metadata::default());
    }

    #[test]
    fn detects_gps_in_a_raw_exif_block() {
        assert!(exif_has_gps(&exif_with_gps()));
        assert!(!exif_has_gps(&exif_orientation_le(1)));
        assert!(!exif_has_gps(b"garbage"));
    }

    #[test]
    fn detects_gps_only_when_present() {
        assert!(!has_gps(&jpeg()));
        assert!(!has_gps(&with_exif(jpeg(), &exif_orientation_le(1))));
        assert!(has_gps(&with_exif(jpeg(), &exif_with_gps())));
    }
}
