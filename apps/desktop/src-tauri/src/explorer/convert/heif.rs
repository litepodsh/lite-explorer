//! HEIC and AVIF input, decoded in pure Rust: no system codecs, same on every platform.
//!
//! HEIC goes through `heic-decoder` (HEVC), AVIF through `avif-decode` (AV1 via rav1d).

use image::{DynamicImage, Rgba, RgbImage, RgbaImage};

use super::metadata::Metadata;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Heic,
    Avif,
}

pub struct Raster {
    pub image: DynamicImage,
    /// Always 1 for HEIC (the decoder applies the container's rotation) and the angle
    /// of the `irot` box, expressed as an EXIF orientation, for AVIF.
    pub orientation: u8,
    pub metadata: Metadata,
}

fn unsupported() -> String {
    "Unsupported or damaged image".to_string()
}

/// Looks at the `ftyp` box: AVIF if any brand says so, HEIC for the HEVC brands.
// `chunks_exact` works on older toolchains than `as_chunks`, which clippy would prefer.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn kind(bytes: &[u8]) -> Option<Kind> {
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let end = size.clamp(16, bytes.len().min(1024));
    let major = &bytes[8..12];
    // Bytes 12..16 are the minor version; compatible brands follow.
    let brands = std::iter::once(major).chain(bytes[16..end].chunks_exact(4));
    let (mut avif, mut heic, mut generic) = (false, false, false);
    for brand in brands {
        match brand {
            b"avif" | b"avis" => avif = true,
            b"heic" | b"heix" | b"hevc" | b"hevx" | b"heim" | b"heis" | b"msf1" => heic = true,
            b"mif1" => generic = true,
            _ => {}
        }
    }
    if avif {
        Some(Kind::Avif)
    } else if heic || generic {
        Some(Kind::Heic)
    } else {
        None
    }
}

/// Payloads that follow each occurrence of a four-character box type. Property boxes such
/// as `ispe` and `irot` are tiny, so a scan is enough to read them.
fn property_payloads<'a>(bytes: &'a [u8], name: &'a [u8; 4]) -> impl Iterator<Item = &'a [u8]> {
    bytes
        .windows(4)
        .enumerate()
        .filter(move |(_, window)| *window == name)
        .map(|(at, _)| &bytes[at + 4..])
}

/// The largest `ispe` (image spatial extents) in the file.
fn avif_size(bytes: &[u8]) -> Option<(u32, u32)> {
    property_payloads(bytes, b"ispe")
        .filter(|payload| payload.len() >= 12)
        .map(|payload| {
            (
                u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]),
                u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]),
            )
        })
        .max_by_key(|(width, height)| *width as u64 * *height as u64)
}

/// `irot` stores a counter-clockwise quarter-turn count; as an EXIF orientation that is
/// 8 (90°), 3 (180°) and 6 (270°).
fn avif_orientation(bytes: &[u8]) -> u8 {
    match property_payloads(bytes, b"irot")
        .next()
        .and_then(|payload| payload.first())
        .map(|angle| angle & 3)
    {
        Some(1) => 8,
        Some(2) => 3,
        Some(3) => 6,
        _ => 1,
    }
}

pub fn has_alpha(bytes: &[u8], kind: Kind) -> bool {
    match kind {
        Kind::Heic => heic_decoder::probe(bytes).is_ok_and(|info| info.alpha_bit_depth.is_some()),
        Kind::Avif => bytes
            .windows(b"auxiliary:alpha".len())
            .any(|window| window == b"auxiliary:alpha"),
    }
}

/// Size after the container's transforms, read without decoding any pixels.
pub fn dimensions(bytes: &[u8], kind: Kind) -> Result<(u32, u32), String> {
    match kind {
        Kind::Heic => {
            let info = heic_decoder::probe(bytes).map_err(|_| unsupported())?;
            Ok((info.width as u32, info.height as u32))
        }
        Kind::Avif => {
            let (width, height) = avif_size(bytes).ok_or_else(unsupported)?;
            // A quarter turn swaps the sides of the decoded picture.
            Ok(if matches!(avif_orientation(bytes), 6 | 8) {
                (height, width)
            } else {
                (width, height)
            })
        }
    }
}

/// HEIC alpha may be premultiplied; the rest of the pipeline expects straight alpha.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn straight_alpha(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

fn decode_heic(bytes: &[u8]) -> Result<Raster, String> {
    let decoded = heic_decoder::decode(bytes).map_err(|_| unsupported())?;
    let (width, height) = (decoded.width as u32, decoded.height as u32);
    let image = if decoded.alpha.is_some() {
        let mut rgba = decoded.to_rgba8().map_err(|_| unsupported())?;
        if rgba.premultiplied {
            straight_alpha(&mut rgba.data);
        }
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(width, height, rgba.data).ok_or_else(unsupported)?,
        )
    } else {
        let rgb = decoded.to_rgb8().map_err(|_| unsupported())?;
        DynamicImage::ImageRgb8(RgbImage::from_raw(width, height, rgb.data).ok_or_else(unsupported)?)
    };
    Ok(Raster {
        image,
        // The container's rotation and mirroring are already applied.
        orientation: 1,
        metadata: Metadata {
            exif: decoded.metadata.exif,
            icc: decoded.metadata.icc_profile,
        },
    })
}

fn decode_avif(bytes: &[u8]) -> Result<Raster, String> {
    use avif_decode::Image;
    let decoded = avif_decode::Decoder::from_avif(bytes)
        .and_then(|decoder| decoder.to_image())
        .map_err(|_| unsupported())?;
    let rgba: RgbaImage = match decoded {
        Image::Rgba8(img) => build(img.width(), img.height(), img.pixels().map(|p| [p.r, p.g, p.b, p.a])),
        Image::Rgb8(img) => build(img.width(), img.height(), img.pixels().map(|p| [p.r, p.g, p.b, 255])),
        Image::Rgba16(img) => build(
            img.width(),
            img.height(),
            img.pixels().map(|p| [(p.r >> 8) as u8, (p.g >> 8) as u8, (p.b >> 8) as u8, (p.a >> 8) as u8]),
        ),
        Image::Rgb16(img) => build(
            img.width(),
            img.height(),
            img.pixels().map(|p| [(p.r >> 8) as u8, (p.g >> 8) as u8, (p.b >> 8) as u8, 255]),
        ),
        Image::Gray8(img) => build(
            img.width(),
            img.height(),
            img.pixels().map(|p| {
                let v = p.value();
                [v, v, v, 255]
            }),
        ),
        Image::Gray16(img) => build(
            img.width(),
            img.height(),
            img.pixels().map(|p| {
                let v = (p.value() >> 8) as u8;
                [v, v, v, 255]
            }),
        ),
    };
    Ok(Raster {
        image: DynamicImage::ImageRgba8(rgba),
        orientation: avif_orientation(bytes),
        // AVIF files rarely carry EXIF, and `avif-decode` does not expose it.
        metadata: Metadata::default(),
    })
}

fn build(width: usize, height: usize, pixels: impl Iterator<Item = [u8; 4]>) -> RgbaImage {
    let mut image = RgbaImage::new(width as u32, height as u32);
    for (target, [r, g, b, a]) in image.pixels_mut().zip(pixels) {
        *target = Rgba([r, g, b, a]);
    }
    image
}

pub fn decode(bytes: &[u8], kind: Kind) -> Result<Raster, String> {
    match kind {
        Kind::Heic => decode_heic(bytes),
        Kind::Avif => decode_avif(bytes),
    }
}

/// EXIF and ICC of a HEIC file, without decoding its pixels.
pub fn heic_metadata(bytes: &[u8]) -> Metadata {
    heic_decoder::read_metadata(bytes)
        .map(|metadata| Metadata {
            exif: metadata.exif,
            icc: metadata.icc_profile,
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::convert::{encode::encode, formats::ImageFormat};

    fn ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8> {
        let size = 16 + compatible.len() * 4;
        let mut data = Vec::new();
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(major);
        data.extend_from_slice(&0u32.to_be_bytes());
        for brand in compatible {
            data.extend_from_slice(*brand);
        }
        data.extend_from_slice(&[0; 32]);
        data
    }

    fn avif_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba([(x * 255 / width) as u8, (y * 255 / height) as u8, 90, 255]);
        }
        encode(&DynamicImage::ImageRgba8(image), ImageFormat::Avif, 90).unwrap()
    }

    #[test]
    fn detects_the_container_from_its_brands() {
        assert_eq!(kind(&ftyp(b"heic", &[b"mif1", b"heic"])), Some(Kind::Heic));
        assert_eq!(kind(&ftyp(b"mif1", &[b"heic"])), Some(Kind::Heic));
        assert_eq!(kind(&ftyp(b"avif", &[b"mif1", b"miaf"])), Some(Kind::Avif));
        // AVIF files often list mif1 first; the avif brand still wins.
        assert_eq!(kind(&ftyp(b"mif1", &[b"avif", b"miaf"])), Some(Kind::Avif));
    }

    #[test]
    fn other_files_are_not_heif() {
        assert_eq!(kind(&ftyp(b"isom", &[b"mp41"])), None);
        assert_eq!(kind(b"\x89PNG\r\n\x1a\n and some more bytes"), None);
        assert_eq!(kind(b"short"), None);
    }

    #[test]
    fn irot_maps_to_exif_orientations() {
        let with_irot = |angle: u8| [b"....irot".as_slice(), &[angle]].concat();
        assert_eq!(avif_orientation(&with_irot(0)), 1);
        assert_eq!(avif_orientation(&with_irot(1)), 8);
        assert_eq!(avif_orientation(&with_irot(2)), 3);
        assert_eq!(avif_orientation(&with_irot(3)), 6);
        assert_eq!(avif_orientation(b"no rotation box"), 1);
    }

    #[test]
    fn avif_round_trip_keeps_size_and_colors() {
        let bytes = avif_bytes(64, 48);
        assert_eq!(kind(&bytes), Some(Kind::Avif));
        assert_eq!(dimensions(&bytes, Kind::Avif).unwrap(), (64, 48));
        let raster = decode(&bytes, Kind::Avif).unwrap();
        assert_eq!((raster.image.width(), raster.image.height()), (64, 48));
        assert_eq!(raster.orientation, 1);
        let pixel = raster.image.to_rgba8().get_pixel(32, 24).0;
        // Lossy, so allow a small error around the gradient's value at that point.
        assert!((pixel[0] as i32 - 127).abs() < 12, "{pixel:?}");
        assert!((pixel[1] as i32 - 127).abs() < 12, "{pixel:?}");
        assert!((pixel[2] as i32 - 90).abs() < 12, "{pixel:?}");
        assert_eq!(pixel[3], 255);
    }

    #[test]
    fn avif_without_alpha_is_reported_opaque() {
        assert!(!has_alpha(&avif_bytes(16, 16), Kind::Avif));
    }

    #[test]
    fn broken_files_are_a_clean_error() {
        let heic = [ftyp(b"heic", &[b"mif1"]), vec![0; 64]].concat();
        assert_eq!(dimensions(&heic, Kind::Heic).unwrap_err(), "Unsupported or damaged image");
        assert_eq!(decode(&heic, Kind::Heic).err().unwrap(), "Unsupported or damaged image");
        let avif = [ftyp(b"avif", &[b"mif1"]), vec![0; 64]].concat();
        assert_eq!(dimensions(&avif, Kind::Avif).unwrap_err(), "Unsupported or damaged image");
        assert_eq!(decode(&avif, Kind::Avif).err().unwrap(), "Unsupported or damaged image");
    }

    #[test]
    fn premultiplied_alpha_is_undone() {
        let mut data = vec![64, 0, 0, 128, 10, 20, 30, 255, 5, 5, 5, 0];
        straight_alpha(&mut data);
        assert_eq!(&data[..4], &[128, 0, 0, 128]);
        assert_eq!(&data[4..8], &[10, 20, 30, 255]);
        assert_eq!(&data[8..], &[5, 5, 5, 0]);
    }

    /// Manual check against a real photo: `LITE_EXPLORER_HEIC_SAMPLE=/path/photo.heic cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn decodes_a_real_heic_photo() {
        let path = std::env::var("LITE_EXPLORER_HEIC_SAMPLE").expect("LITE_EXPLORER_HEIC_SAMPLE");
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(kind(&bytes), Some(Kind::Heic));
        let (width, height) = dimensions(&bytes, Kind::Heic).unwrap();
        let raster = decode(&bytes, Kind::Heic).unwrap();
        assert_eq!((raster.image.width(), raster.image.height()), (width, height));
        assert_eq!(raster.orientation, 1);
        assert!(raster.metadata.exif.is_some());
    }
}
