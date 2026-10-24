use std::io::Cursor;

use image::{
    codecs::{avif::AvifEncoder, jpeg::JpegEncoder},
    imageops::FilterType, DynamicImage, ExtendedColorType,
    ImageEncoder, ImageFormat as OutputFormat, Rgb, RgbImage,
};

use super::formats::ImageFormat;

const ICO_MAX_SIDE: u32 = 256;
/// From this quality up, JPEG keeps the chroma channels at full resolution (4:4:4).
const JPEG_FULL_CHROMA_QUALITY: u8 = 90;
/// 1 is slowest and smallest, 10 is fastest. 8 keeps the live preview responsive.
const AVIF_SPEED: u8 = 8;

/// Composites transparency over white, for formats without an alpha channel.
fn flatten_on_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let mut flat = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = pixel[3] as u32;
        let blend = |channel: u8| ((channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        flat.put_pixel(x, y, Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]));
    }
    flat
}

pub fn encode(image: &DynamicImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>, String> {
    let quality = quality.clamp(1, 100);
    let mut output = Cursor::new(Vec::new());
    match format {
        ImageFormat::Jpeg => {
            let flat = flatten_on_white(image);
            if flat.width() <= u16::MAX as u32 && flat.height() <= u16::MAX as u32 {
                let mut encoder = jpeg_encoder::Encoder::new(output.get_mut(), quality);
                // Full-resolution chroma keeps colored edges (vector art, text) sharp.
                if quality >= JPEG_FULL_CHROMA_QUALITY {
                    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::R_4_4_4);
                }
                encoder
                    .encode(
                        flat.as_raw(),
                        flat.width() as u16,
                        flat.height() as u16,
                        jpeg_encoder::ColorType::Rgb,
                    )
                    .map_err(|error| error.to_string())?;
            } else {
                // The JPEG format itself stops at 65535 pixels per side.
                JpegEncoder::new_with_quality(&mut output, quality)
                    .write_image(flat.as_raw(), flat.width(), flat.height(), ExtendedColorType::Rgb8)
                    .map_err(|error| error.to_string())?;
            }
        }
        ImageFormat::Avif => {
            let rgba = image.to_rgba8();
            AvifEncoder::new_with_speed_quality(&mut output, AVIF_SPEED, quality)
                .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
                .map_err(|error| error.to_string())?;
        }
        ImageFormat::Webp => {
            let rgba = image.to_rgba8();
            let encoder = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height());
            let memory = if quality == 100 {
                encoder.encode_lossless()
            } else {
                encoder.encode(quality as f32)
            };
            return Ok(memory.to_vec());
        }
        ImageFormat::Bmp => {
            DynamicImage::ImageRgb8(flatten_on_white(image))
                .write_to(&mut output, OutputFormat::Bmp)
                .map_err(|error| error.to_string())?;
        }
        ImageFormat::Ico => {
            let fitted = if image.width() > ICO_MAX_SIDE || image.height() > ICO_MAX_SIDE {
                image.resize(ICO_MAX_SIDE, ICO_MAX_SIDE, FilterType::Lanczos3)
            } else {
                image.clone()
            };
            DynamicImage::ImageRgba8(fitted.to_rgba8())
                .write_to(&mut output, OutputFormat::Ico)
                .map_err(|error| error.to_string())?;
        }
        ImageFormat::Png => write_rgba(image, &mut output, OutputFormat::Png)?,
        ImageFormat::Gif => write_rgba(image, &mut output, OutputFormat::Gif)?,
        ImageFormat::Tiff => write_rgba(image, &mut output, OutputFormat::Tiff)?,
    }
    Ok(output.into_inner())
}

fn write_rgba(
    image: &DynamicImage,
    output: &mut Cursor<Vec<u8>>,
    format: OutputFormat,
) -> Result<(), String> {
    DynamicImage::ImageRgba8(image.to_rgba8())
        .write_to(output, format)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// Deterministic noise, so quality changes the size.
    fn noisy(width: u32, height: u32) -> DynamicImage {
        let mut state: u32 = 12345;
        let mut image = RgbaImage::new(width, height);
        for pixel in image.pixels_mut() {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let [a, b, c, _] = state.to_le_bytes();
            *pixel = Rgba([a, b, c, 255]);
        }
        DynamicImage::ImageRgba8(image)
    }

    #[test]
    fn every_writable_format_round_trips_dimensions() {
        let source = noisy(40, 30);
        for format in ImageFormat::WRITABLE {
            let bytes = encode(&source, format, 80).unwrap_or_else(|e| panic!("{format:?}: {e}"));
            if format == ImageFormat::Avif {
                // There is no AVIF decoder in the build yet, so check the container instead.
                assert_eq!(&bytes[4..12], b"ftypavif");
                continue;
            }
            let decoded = image::load_from_memory(&bytes).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (40, 30), "{format:?}");
        }
    }

    #[test]
    fn lower_quality_gives_smaller_jpeg_and_webp() {
        let source = noisy(96, 96);
        for format in [ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Avif] {
            let low = encode(&source, format, 20).unwrap().len();
            let high = encode(&source, format, 95).unwrap().len();
            assert!(low < high, "{format:?}: {low} !< {high}");
        }
    }

    #[test]
    fn jpeg_flattens_transparency_onto_white() {
        let mut transparent = RgbaImage::new(8, 8);
        for pixel in transparent.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]);
        }
        let bytes = encode(&DynamicImage::ImageRgba8(transparent), ImageFormat::Jpeg, 95).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgb8();
        let [r, g, b] = decoded.get_pixel(4, 4).0;
        assert!(r > 240 && g > 240 && b > 240, "{r} {g} {b}");
    }

    #[test]
    fn ico_is_limited_to_256_pixels() {
        let bytes = encode(&noisy(512, 300), ImageFormat::Ico, 80).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert!(decoded.width() <= 256 && decoded.height() <= 256);
    }

    #[test]
    fn quality_is_clamped() {
        let source = noisy(16, 16);
        assert!(encode(&source, ImageFormat::Jpeg, 0).is_ok());
        assert!(encode(&source, ImageFormat::Jpeg, 255).is_ok());
    }

    #[test]
    fn high_quality_jpeg_keeps_one_pixel_color_edges() {
        // Alternating pure red and pure blue columns: any chroma averaging turns them purple.
        let mut stripes = RgbaImage::new(16, 16);
        for (x, _, pixel) in stripes.enumerate_pixels_mut() {
            *pixel = if x % 2 == 0 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) };
        }
        let source = DynamicImage::ImageRgba8(stripes);
        let decode = |quality| {
            let bytes = encode(&source, ImageFormat::Jpeg, quality).unwrap();
            image::load_from_memory(&bytes).unwrap().to_rgb8()
        };
        let sharp = decode(100);
        let [r, _, b] = sharp.get_pixel(8, 8).0;
        assert!(r > 200 && b < 60, "red column became {r} / {b}");
        let [r, _, b] = sharp.get_pixel(9, 8).0;
        assert!(b > 200 && r < 60, "blue column became {r} / {b}");
    }

    #[test]
    fn jpeg_quality_still_controls_the_size() {
        let source = noisy(96, 96);
        let low = encode(&source, ImageFormat::Jpeg, 30).unwrap().len();
        let high = encode(&source, ImageFormat::Jpeg, 95).unwrap().len();
        assert!(low < high, "{low} !< {high}");
    }
}
