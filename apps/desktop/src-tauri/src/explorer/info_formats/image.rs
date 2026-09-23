//! Header-only facts for raster and vector images; nothing is decoded.
use super::{be16, be32, le16, le24, le32, InfoSection, Kind};
use crate::preview::xml;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

const METRES_PER_INCH: f64 = 0.0254;
const PNG_MAX_CHUNKS: usize = 64;
const JPEG_MAX_SEGMENTS: usize = 256;
const TIFF_MAX_ENTRIES: usize = 512;
const SVG_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
struct Facts {
    size: Option<(u32, u32)>,
    view_box: Option<String>,
    depth: Option<u16>,
    color: Option<&'static str>,
    alpha: Option<bool>,
    animated: bool,
    dpi: Option<(f64, f64)>,
}

impl Facts {
    fn into_section(self) -> Option<InfoSection> {
        let mut section = InfoSection::new("Image");
        match (self.size, self.view_box) {
            (Some((width, height)), _) if width > 0 && height > 0 => {
                section.push("Dimensions", format!("{width} × {height} pixels"))
            }
            (_, Some(view_box)) => section.push("Dimensions", format!("viewBox {view_box}")),
            _ => {}
        }
        section.push_opt(
            "Bit depth",
            self.depth
                .filter(|depth| *depth > 0)
                .map(|depth| depth.to_string()),
        );
        section.push_opt("Color model", self.color);
        section.push_opt(
            "Alpha",
            self.alpha.map(|alpha| if alpha { "Yes" } else { "No" }),
        );
        if self.animated {
            section.push("Animated", "Yes");
        }
        if let Some((x, y)) = self.dpi.filter(|(x, y)| *x >= 1.0 && *y >= 1.0) {
            section.push(
                "Resolution",
                if (x - y).abs() < 0.5 {
                    format!("{x:.0} dpi")
                } else {
                    format!("{x:.0} × {y:.0} dpi")
                },
            );
        }
        section.non_empty()
    }
}

pub(crate) fn section(kind: Kind, head: &[u8], file: &mut File) -> Option<InfoSection> {
    let facts = match kind {
        Kind::Png => png(head, file),
        Kind::Gif => gif(head),
        Kind::Jpeg => jpeg(file),
        Kind::Webp => webp(head),
        Kind::Bmp => bmp(head),
        Kind::Tiff => tiff(head, file),
        Kind::Psd => psd(head),
        Kind::Svg => svg(file),
        _ => None,
    }?;
    facts.into_section()
}

fn png(head: &[u8], file: &mut File) -> Option<Facts> {
    if head.get(12..16) != Some(&b"IHDR"[..]) {
        return None;
    }
    let color = *head.get(25)?;
    let mut facts = Facts {
        size: Some((be32(head, 16)?, be32(head, 20)?)),
        depth: Some(u16::from(*head.get(24)?)),
        color: Some(match color {
            0 => "Grayscale",
            2 => "RGB",
            3 => "Indexed",
            4 => "Grayscale + alpha",
            6 => "RGBA",
            _ => "Unknown",
        }),
        alpha: Some(matches!(color, 4 | 6)),
        ..Facts::default()
    };
    let mut offset = 8u64;
    for _ in 0..PNG_MAX_CHUNKS {
        let mut header = [0u8; 8];
        if file.seek(SeekFrom::Start(offset)).is_err() || file.read_exact(&mut header).is_err() {
            break;
        }
        let length = u64::from(u32::from_be_bytes([
            header[0], header[1], header[2], header[3],
        ]));
        match &header[4..8] {
            b"IDAT" | b"IEND" => break,
            b"acTL" => facts.animated = true,
            b"tRNS" => facts.alpha = Some(true),
            b"pHYs" if length >= 9 => {
                let mut data = [0u8; 9];
                // Unit 1 is pixels per metre; 0 only gives an aspect ratio.
                if file.read_exact(&mut data).is_ok() && data[8] == 1 {
                    let x = f64::from(u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
                    let y = f64::from(u32::from_be_bytes([data[4], data[5], data[6], data[7]]));
                    facts.dpi = Some((x * METRES_PER_INCH, y * METRES_PER_INCH));
                }
            }
            _ => {}
        }
        offset = offset.saturating_add(12).saturating_add(length);
    }
    Some(facts)
}

fn gif(head: &[u8]) -> Option<Facts> {
    Some(Facts {
        size: Some((u32::from(le16(head, 6)?), u32::from(le16(head, 8)?))),
        color: Some("Indexed"),
        ..Facts::default()
    })
}

fn is_start_of_frame(marker: u8) -> bool {
    matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
}

/// Walks the segments before the image data: JFIF gives the density, a start-of-frame the size.
fn jpeg(file: &mut File) -> Option<Facts> {
    let mut facts = Facts::default();
    file.seek(SeekFrom::Start(2)).ok()?;
    for _ in 0..JPEG_MAX_SEGMENTS {
        let mut marker = [0u8; 2];
        if file.read_exact(&mut marker).is_err() || marker[0] != 0xff {
            break;
        }
        while marker[1] == 0xff {
            if file.read_exact(&mut marker[1..]).is_err() {
                return Some(facts);
            }
        }
        if matches!(marker[1], 0xda | 0xd9) {
            break;
        }
        if matches!(marker[1], 0x01 | 0xd0..=0xd8) {
            continue;
        }
        let mut size = [0u8; 2];
        if file.read_exact(&mut size).is_err() {
            break;
        }
        let size = u16::from_be_bytes(size);
        if size < 2 {
            break;
        }
        if marker[1] != 0xe0 && !is_start_of_frame(marker[1]) {
            if file.seek(SeekFrom::Current(i64::from(size) - 2)).is_err() {
                break;
            }
            continue;
        }
        let mut payload = vec![0u8; usize::from(size - 2)];
        if file.read_exact(&mut payload).is_err() {
            break;
        }
        if marker[1] == 0xe0 {
            if payload.starts_with(b"JFIF\0") {
                let x = f64::from(be16(&payload, 8).unwrap_or(0));
                let y = f64::from(be16(&payload, 10).unwrap_or(0));
                facts.dpi = match payload.get(7) {
                    Some(1) => Some((x, y)),
                    Some(2) => Some((x * 2.54, y * 2.54)),
                    _ => None,
                };
            }
            continue;
        }
        if let (Some(height), Some(width)) = (be16(&payload, 1), be16(&payload, 3)) {
            facts.size = Some((u32::from(width), u32::from(height)));
        }
        facts.depth = payload.first().map(|depth| u16::from(*depth));
        facts.color = payload.get(5).map(|components| match components {
            1 => "Grayscale",
            3 => "YCbCr",
            4 => "CMYK",
            _ => "Unknown",
        });
        break;
    }
    Some(facts)
}

fn webp(head: &[u8]) -> Option<Facts> {
    let mut facts = Facts {
        color: Some("RGB"),
        ..Facts::default()
    };
    match head.get(12..16)? {
        b"VP8 " => {
            if head.get(23..26) != Some(&[0x9d, 0x01, 0x2a][..]) {
                return None;
            }
            facts.size = Some((
                u32::from(le16(head, 26)? & 0x3fff),
                u32::from(le16(head, 28)? & 0x3fff),
            ));
            facts.alpha = Some(false);
        }
        b"VP8L" => {
            if head.get(20) != Some(&0x2f) {
                return None;
            }
            let bits = le32(head, 21)?;
            facts.size = Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
            facts.alpha = Some((bits >> 28) & 1 == 1);
        }
        b"VP8X" => {
            let flags = *head.get(20)?;
            facts.size = Some((le24(head, 24)? + 1, le24(head, 27)? + 1));
            facts.alpha = Some(flags & 0x10 != 0);
            facts.animated = flags & 0x02 != 0;
        }
        _ => return None,
    }
    Some(facts)
}

fn bmp(head: &[u8]) -> Option<Facts> {
    let header_size = le32(head, 14)?;
    let (width, height, depth) = if header_size == 12 {
        (
            u32::from(le16(head, 18)?),
            u32::from(le16(head, 20)?),
            le16(head, 24)?,
        )
    } else {
        // A negative height marks a top-down bitmap.
        (
            (le32(head, 18)? as i32).unsigned_abs(),
            (le32(head, 22)? as i32).unsigned_abs(),
            le16(head, 28)?,
        )
    };
    let mut facts = Facts {
        size: Some((width, height)),
        depth: Some(depth),
        color: Some(if depth <= 8 { "Indexed" } else { "RGB" }),
        ..Facts::default()
    };
    if header_size >= 40 {
        let compression = le32(head, 30)?;
        let alpha_mask = if header_size >= 56 {
            le32(head, 66).unwrap_or(0)
        } else {
            0
        };
        facts.alpha = Some(depth == 32 && (alpha_mask != 0 || compression == 6));
        let (x, y) = (f64::from(le32(head, 38)?), f64::from(le32(head, 42)?));
        facts.dpi = Some((x * METRES_PER_INCH, y * METRES_PER_INCH));
    }
    Some(facts)
}

fn read_at(file: &mut File, offset: u64, length: usize) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0u8; length];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// Reads the first IFD: size, depth, photometric interpretation and resolution.
fn tiff(head: &[u8], file: &mut File) -> Option<Facts> {
    let little = head.starts_with(b"II");
    let u16_at = |bytes: &[u8], at: usize| {
        if little {
            le16(bytes, at)
        } else {
            be16(bytes, at)
        }
    };
    let u32_at = |bytes: &[u8], at: usize| {
        if little {
            le32(bytes, at)
        } else {
            be32(bytes, at)
        }
    };
    let ifd = u64::from(u32_at(head, 4)?);
    let count = usize::from(u16_at(&read_at(file, ifd, 2)?, 0)?).min(TIFF_MAX_ENTRIES);
    let entries = read_at(file, ifd.saturating_add(2), count * 12)?;
    let mut facts = Facts::default();
    let (mut width, mut height, mut x_res, mut y_res, mut unit) = (None, None, None, None, 2u32);
    for entry in entries.as_chunks::<12>().0 {
        let entry = &entry[..];
        let (Some(tag), Some(kind), Some(n)) =
            (u16_at(entry, 0), u16_at(entry, 2), u32_at(entry, 4))
        else {
            continue;
        };
        let pointer = u64::from(u32_at(entry, 8).unwrap_or(0));
        let number = match kind {
            3 if n <= 2 => u16_at(entry, 8).map(f64::from),
            3 => read_at(file, pointer, 2)
                .and_then(|bytes| u16_at(&bytes, 0))
                .map(f64::from),
            4 if n <= 1 => u32_at(entry, 8).map(f64::from),
            5 => read_at(file, pointer, 8).and_then(|bytes| {
                let numerator = u32_at(&bytes, 0)?;
                let denominator = u32_at(&bytes, 4)?;
                (denominator != 0).then(|| f64::from(numerator) / f64::from(denominator))
            }),
            _ => None,
        };
        let Some(number) = number else { continue };
        match tag {
            256 => width = Some(number as u32),
            257 => height = Some(number as u32),
            258 => facts.depth = Some(number as u16),
            262 => {
                facts.color = Some(match number as u32 {
                    0 | 1 => "Grayscale",
                    2 => "RGB",
                    3 => "Indexed",
                    5 => "CMYK",
                    6 => "YCbCr",
                    _ => "Unknown",
                })
            }
            282 => x_res = Some(number),
            283 => y_res = Some(number),
            296 => unit = number as u32,
            _ => {}
        }
    }
    if let (Some(width), Some(height)) = (width, height) {
        facts.size = Some((width, height));
    }
    let scale = match unit {
        2 => Some(1.0),
        3 => Some(2.54),
        _ => None,
    };
    if let (Some(x), Some(y), Some(scale)) = (x_res, y_res, scale) {
        facts.dpi = Some((x * scale, y * scale));
    }
    Some(facts)
}

fn psd(head: &[u8]) -> Option<Facts> {
    let channels = be16(head, 12)?;
    let (color, base_channels) = match be16(head, 24)? {
        0 => ("Bitmap", 1),
        1 => ("Grayscale", 1),
        2 => ("Indexed", 1),
        3 => ("RGB", 3),
        4 => ("CMYK", 4),
        7 => ("Multichannel", channels),
        8 => ("Duotone", 1),
        9 => ("Lab", 3),
        _ => ("Unknown", channels),
    };
    Some(Facts {
        size: Some((be32(head, 18)?, be32(head, 14)?)),
        depth: Some(be16(head, 22)?),
        color: Some(color),
        alpha: Some(channels > base_channels),
        ..Facts::default()
    })
}

fn svg(file: &mut File) -> Option<Facts> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut bytes = Vec::new();
    (&mut *file)
        .take(SVG_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let document = xml::parse_document(&String::from_utf8_lossy(&bytes));
    let root = document.children_named("svg").next()?;
    let mut facts = Facts::default();
    match (
        root.attr("width").and_then(svg_length),
        root.attr("height").and_then(svg_length),
    ) {
        (Some(width), Some(height)) => {
            facts.size = Some((width.round() as u32, height.round() as u32))
        }
        _ => {
            facts.view_box = root
                .attr("viewBox")
                .map(|value| {
                    value
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|value| !value.is_empty());
        }
    }
    Some(facts)
}

/// An absolute SVG length in CSS pixels; relative units (`%`, `em`) give `None`.
fn svg_length(value: &str) -> Option<f64> {
    let value = value.trim();
    let split = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale = match unit.trim() {
        "" | "px" => 1.0,
        "pt" => 96.0 / 72.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        _ => return None,
    };
    (number > 0.0).then_some(number * scale)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{rows, rows_for};

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend(kind);
        out.extend(data);
        out.extend([0; 4]);
        out
    }

    fn png(color: u8, chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = 640u32.to_be_bytes().to_vec();
        ihdr.extend(480u32.to_be_bytes());
        ihdr.extend([8, color, 0, 0, 0]);
        out.extend(chunk(b"IHDR", &ihdr));
        for extra in chunks {
            out.extend(extra);
        }
        out.extend(chunk(b"IDAT", &[]));
        out
    }

    #[test]
    fn png_header_density_and_animation() {
        let mut phys = 11811u32.to_be_bytes().to_vec();
        phys.extend(11811u32.to_be_bytes());
        phys.push(1);
        let bytes = png(6, &[chunk(b"acTL", &[0; 8]), chunk(b"pHYs", &phys)]);
        assert_eq!(
            rows_for(&bytes, "anim.png", "Image"),
            rows(&[
                ("Dimensions", "640 × 480 pixels"),
                ("Bit depth", "8"),
                ("Color model", "RGBA"),
                ("Alpha", "Yes"),
                ("Animated", "Yes"),
                ("Resolution", "300 dpi"),
            ])
        );
        let rgb = rows_for(&png(2, &[]), "plain.png", "Image");
        assert!(rgb.contains(&("Alpha".into(), "No".into())));
        let transparent = rows_for(&png(2, &[chunk(b"tRNS", &[0; 6])]), "t.png", "Image");
        assert!(transparent.contains(&("Alpha".into(), "Yes".into())));
    }

    #[test]
    fn png_with_huge_chunk_length_keeps_header_facts() {
        let mut bytes = png(2, &[]);
        let idat = bytes.len() - 12;
        bytes[idat..idat + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        bytes[idat + 4..idat + 8].copy_from_slice(b"zzzz");
        let found = rows_for(&bytes, "odd.png", "Image");
        assert_eq!(found[0], ("Dimensions".into(), "640 × 480 pixels".into()));
    }

    #[test]
    fn gif_and_jpeg_headers() {
        assert_eq!(
            rows_for(b"GIF89a\x80\x02\xe0\x01", "a.gif", "Image"),
            rows(&[
                ("Dimensions", "640 × 480 pixels"),
                ("Color model", "Indexed"),
            ])
        );
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
        jpeg.extend(b"JFIF\0");
        jpeg.extend([1, 1, 1, 0x01, 0x2c, 0x01, 0x2c, 0, 0]);
        jpeg.extend([0xff, 0xc0, 0x00, 0x11, 8, 0x01, 0xe0, 0x02, 0x80, 3]);
        jpeg.extend([0; 9]);
        assert_eq!(
            rows_for(&jpeg, "a.jpg", "Image"),
            rows(&[
                ("Dimensions", "640 × 480 pixels"),
                ("Bit depth", "8"),
                ("Color model", "YCbCr"),
                ("Resolution", "300 dpi"),
            ])
        );
        assert!(rows_for(b"\xff\xd8\xff\xc0", "cut.jpg", "Image").is_empty());
    }

    fn riff_webp(chunk: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = b"RIFF\0\0\0\0WEBP".to_vec();
        out.extend(chunk);
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
        out
    }

    #[test]
    fn webp_variants() {
        let mut vp8x = vec![0x12, 0, 0, 0];
        vp8x.extend(&639u32.to_le_bytes()[..3]);
        vp8x.extend(&479u32.to_le_bytes()[..3]);
        assert_eq!(
            rows_for(&riff_webp(b"VP8X", &vp8x), "a.webp", "Image"),
            rows(&[
                ("Dimensions", "640 × 480 pixels"),
                ("Color model", "RGB"),
                ("Alpha", "Yes"),
                ("Animated", "Yes"),
            ])
        );
        let bits: u32 = 99 | (49 << 14) | (1 << 28);
        let mut vp8l = vec![0x2f];
        vp8l.extend(bits.to_le_bytes());
        let lossless = rows_for(&riff_webp(b"VP8L", &vp8l), "b.webp", "Image");
        assert_eq!(lossless[0], ("Dimensions".into(), "100 × 50 pixels".into()));
        assert!(lossless.contains(&("Alpha".into(), "Yes".into())));
        let mut vp8 = vec![0, 0, 0, 0x9d, 0x01, 0x2a];
        vp8.extend(320u16.to_le_bytes());
        vp8.extend(240u16.to_le_bytes());
        let lossy = rows_for(&riff_webp(b"VP8 ", &vp8), "c.webp", "Image");
        assert_eq!(lossy[0], ("Dimensions".into(), "320 × 240 pixels".into()));
        assert!(lossy.contains(&("Alpha".into(), "No".into())));
    }

    #[test]
    fn bmp_header() {
        let mut bmp = b"BM".to_vec();
        bmp.extend([0; 8]);
        bmp.extend(54u32.to_le_bytes());
        bmp.extend(40u32.to_le_bytes());
        bmp.extend(2i32.to_le_bytes());
        bmp.extend((-3i32).to_le_bytes());
        bmp.extend(1u16.to_le_bytes());
        bmp.extend(24u16.to_le_bytes());
        bmp.extend(0u32.to_le_bytes());
        bmp.extend(0u32.to_le_bytes());
        bmp.extend(2835u32.to_le_bytes());
        bmp.extend(2835u32.to_le_bytes());
        bmp.extend([0; 8]);
        assert_eq!(
            rows_for(&bmp, "a.bmp", "Image"),
            rows(&[
                ("Dimensions", "2 × 3 pixels"),
                ("Bit depth", "24"),
                ("Color model", "RGB"),
                ("Alpha", "No"),
                ("Resolution", "72 dpi"),
            ])
        );
    }

    fn tiff(little: bool) -> Vec<u8> {
        let u16b = |value: u16| {
            if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            }
        };
        let u32b = |value: u32| {
            if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            }
        };
        let entries: [(u16, u16, u32); 7] = [
            (256, 3, 640),
            (257, 4, 480),
            (258, 3, 8),
            (262, 3, 2),
            (282, 5, 0),
            (283, 5, 0),
            (296, 3, 2),
        ];
        let data_offset = 8 + 2 + entries.len() as u32 * 12 + 4;
        let mut out = if little {
            b"II*\0".to_vec()
        } else {
            b"MM\0*".to_vec()
        };
        out.extend(u32b(8));
        out.extend(u16b(entries.len() as u16));
        for (tag, kind, value) in entries {
            out.extend(u16b(tag));
            out.extend(u16b(kind));
            out.extend(u32b(1));
            match kind {
                3 => {
                    out.extend(u16b(value as u16));
                    out.extend([0, 0]);
                }
                5 => out.extend(u32b(data_offset)),
                _ => out.extend(u32b(value)),
            }
        }
        out.extend(u32b(0));
        out.extend(u32b(300));
        out.extend(u32b(1));
        out
    }

    #[test]
    fn tiff_both_byte_orders_and_raw_skip() {
        let expected = rows(&[
            ("Dimensions", "640 × 480 pixels"),
            ("Bit depth", "8"),
            ("Color model", "RGB"),
            ("Resolution", "300 dpi"),
        ]);
        assert_eq!(rows_for(&tiff(true), "scan.tif", "Image"), expected);
        assert_eq!(rows_for(&tiff(false), "scan.tiff", "Image"), expected);
        assert!(rows_for(&tiff(true), "photo.dng", "Image").is_empty());
    }

    #[test]
    fn psd_header() {
        let mut psd = b"8BPS".to_vec();
        psd.extend(1u16.to_be_bytes());
        psd.extend([0; 6]);
        psd.extend(4u16.to_be_bytes());
        psd.extend(480u32.to_be_bytes());
        psd.extend(640u32.to_be_bytes());
        psd.extend(8u16.to_be_bytes());
        psd.extend(3u16.to_be_bytes());
        assert_eq!(
            rows_for(&psd, "art.psd", "Image"),
            rows(&[
                ("Dimensions", "640 × 480 pixels"),
                ("Bit depth", "8"),
                ("Color model", "RGB"),
                ("Alpha", "Yes"),
            ])
        );
    }

    #[test]
    fn svg_size_or_view_box() {
        let sized = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="2in" height="96px" viewBox="0 0 10 10"/>"#;
        assert_eq!(
            rows_for(sized, "logo.svg", "Image"),
            rows(&[("Dimensions", "192 × 96 pixels")])
        );
        let relative =
            br#"<svg xmlns="http://www.w3.org/2000/svg" width="100%" viewBox="0,0  24 24"></svg>"#;
        assert_eq!(
            rows_for(relative, "icon.svg", "Image"),
            rows(&[("Dimensions", "viewBox 0 0 24 24")])
        );
        assert!(rows_for(b"<html></html>", "fake.svg", "Image").is_empty());
    }
}
