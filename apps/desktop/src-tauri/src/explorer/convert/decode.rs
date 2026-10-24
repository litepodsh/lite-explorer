use std::{fs, io::Cursor, path::Path};

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use super::{heif, metadata::{self, Metadata}, svg};

/// Larger images are rejected before decoding to avoid exhausting memory.
pub const MAX_PIXELS: u64 = 100_000_000;

pub struct Decoded {
    pub image: DynamicImage,
    /// EXIF orientation 1 to 8. 1 when absent or unreadable.
    pub orientation: u8,
    /// EXIF and ICC of the source, copied to the output unless the user strips them.
    pub metadata: Metadata,
}

/// What can be known about an image without decoding its pixels.
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    /// A GIF with more than one frame.
    pub animated: bool,
}

fn check_pixels(width: u32, height: u32) -> Result<(), String> {
    if width as u64 * height as u64 > MAX_PIXELS {
        return Err("Image too large (over 100 MP)".into());
    }
    Ok(())
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, String> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())
}

fn orientation_of(bytes: &[u8]) -> u8 {
    exif::Reader::new()
        .read_from_container(&mut Cursor::new(bytes))
        .ok()
        .and_then(|data| {
            data.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|field| field.value.get_uint(0))
        })
        .filter(|value| (1..=8).contains(value))
        .map_or(1, |value| value as u8)
}

fn decode_owned(bytes: Vec<u8>, vector_scale: Option<f32>) -> Result<Decoded, String> {
    if svg::is_svg(&bytes) {
        let (width, height) = svg::size(&bytes, vector_scale)?;
        check_pixels(width, height)?;
        let image = DynamicImage::ImageRgba8(svg::render(&bytes, vector_scale)?);
        return Ok(Decoded {
            image,
            orientation: 1,
            metadata: Metadata::default(),
        });
    }
    if let Some(kind) = heif::kind(&bytes) {
        let (width, height) = heif::dimensions(&bytes, kind)?;
        check_pixels(width, height)?;
        let raster = heif::decode(&bytes, kind)?;
        return Ok(Decoded {
            image: raster.image,
            orientation: raster.orientation,
            metadata: raster.metadata,
        });
    }
    let (width, height) = reader(&bytes)?
        .into_dimensions()
        .map_err(|_| "Unsupported or damaged image".to_string())?;
    check_pixels(width, height)?;
    let image = reader(&bytes)?
        .decode()
        .map_err(|_| "Unsupported or damaged image".to_string())?;
    Ok(Decoded {
        image,
        orientation: orientation_of(&bytes),
        metadata: metadata::read(&bytes),
    })
}

#[cfg(test)]
/// Decodes at 100 % for vector sources.
pub fn decode_bytes(bytes: &[u8]) -> Result<Decoded, String> {
    decode_owned(bytes.to_vec(), Some(1.0))
}

/// `vector_scale` only matters for SVG: `Some(1.0)` is 100 %, `None` is automatic.
pub fn decode_file(path: &Path, vector_scale: Option<f32>) -> Result<Decoded, String> {
    decode_owned(fs::read(path).map_err(|error| error.to_string())?, vector_scale)
}

/// EXIF and ICC of any readable file, without decoding its pixels.
pub fn metadata_of(bytes: &[u8]) -> Metadata {
    match heif::kind(bytes) {
        Some(heif::Kind::Heic) => heif::heic_metadata(bytes),
        Some(heif::Kind::Avif) => Metadata::default(),
        None => metadata::read(bytes),
    }
}

pub fn probe(bytes: &[u8]) -> Result<Probe, String> {
    if let Some(kind) = heif::kind(bytes) {
        let (width, height) = heif::dimensions(bytes, kind)?;
        return Ok(Probe {
            width,
            height,
            has_alpha: heif::has_alpha(bytes, kind),
            animated: false,
        });
    }
    if svg::is_svg(bytes) {
        // The intrinsic (100 %) size, which the dialog uses to show the output size.
        let (width, height) = svg::size(bytes, Some(1.0))?;
        return Ok(Probe {
            width,
            height,
            has_alpha: true,
            animated: false,
        });
    }
    let source = reader(bytes)?;
    let is_gif = source.format() == Some(ImageFormat::Gif);
    let decoder = source
        .into_decoder()
        .map_err(|_| "Unsupported or damaged image".to_string())?;
    let (width, height) = decoder.dimensions();
    let has_alpha = decoder.color_type().has_alpha();
    let animated = is_gif && gif_frame_count(bytes) > 1;
    Ok(Probe {
        width,
        height,
        has_alpha,
        animated,
    })
}

fn gif_frame_count(bytes: &[u8]) -> usize {
    use image::{codecs::gif::GifDecoder, AnimationDecoder};
    GifDecoder::new(Cursor::new(bytes))
        .map(|decoder| decoder.into_frames().take(2).count())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
            .write_to(&mut output, ImageFormat::Png)
            .unwrap();
        output.into_inner()
    }

    #[test]
    fn decodes_by_content_not_by_name() {
        let decoded = decode_bytes(&png_bytes(7, 5)).unwrap();
        assert_eq!((decoded.image.width(), decoded.image.height()), (7, 5));
    }

    #[test]
    fn orientation_defaults_to_one_without_exif() {
        assert_eq!(decode_bytes(&png_bytes(2, 2)).unwrap().orientation, 1);
    }

    #[test]
    fn garbage_is_a_clean_error() {
        let error = decode_bytes(b"this is not an image").err().unwrap();
        assert_eq!(error, "Unsupported or damaged image");
    }

    #[test]
    fn rejects_images_over_the_pixel_limit() {
        assert!(check_pixels(10_000, 10_000).is_ok());
        assert_eq!(
            check_pixels(10_001, 10_000).unwrap_err(),
            "Image too large (over 100 MP)"
        );
    }

    #[test]
    fn probe_reports_size_and_alpha_without_decoding_pixels() {
        let probe = probe(&png_bytes(7, 5)).unwrap();
        assert_eq!((probe.width, probe.height), (7, 5));
        assert!(probe.has_alpha);
        assert!(!probe.animated);
    }

    #[test]
    fn svg_is_rasterized_when_decoded_and_probed() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="#0000ff"/></svg>"##;
        let decoded = decode_bytes(svg).unwrap();
        assert_eq!((decoded.image.width(), decoded.image.height()), (100, 50));
        assert_eq!(decoded.orientation, 1);
        let scaled = decode_owned(svg.to_vec(), Some(4.0)).unwrap();
        assert_eq!((scaled.image.width(), scaled.image.height()), (400, 200));
        let probe = probe(svg).unwrap();
        assert_eq!((probe.width, probe.height), (100, 50));
        assert!(probe.has_alpha);
    }

    #[test]
    fn decoded_reads_metadata_from_the_source() {
        use crate::explorer::convert::{
            encode::encode,
            formats::ImageFormat as Format,
            metadata::{exif_orientation_le, with_exif},
        };
        let jpeg = encode(
            &DynamicImage::ImageRgba8(RgbaImage::new(8, 8)),
            Format::Jpeg,
            80,
        )
        .unwrap();
        let source = with_exif(jpeg, &exif_orientation_le(6));
        let decoded = decode_bytes(&source).unwrap();
        assert_eq!(decoded.orientation, 6);
        assert_eq!(
            decoded.metadata.exif.as_deref(),
            Some(exif_orientation_le(6).as_slice())
        );
        assert_eq!(decode_bytes(&png_bytes(3, 3)).unwrap().metadata, Metadata::default());
    }

    #[test]
    fn avif_is_decoded_and_probed_through_the_heif_path() {
        use crate::explorer::convert::{encode::encode, formats::ImageFormat as Format};
        let source = encode(
            &DynamicImage::ImageRgba8(RgbaImage::from_pixel(40, 30, image::Rgba([200, 50, 50, 255]))),
            Format::Avif,
            80,
        )
        .unwrap();
        let decoded = decode_bytes(&source).unwrap();
        assert_eq!((decoded.image.width(), decoded.image.height()), (40, 30));
        let probe = probe(&source).unwrap();
        assert_eq!((probe.width, probe.height), (40, 30));
        assert!(!probe.has_alpha);
    }
}
