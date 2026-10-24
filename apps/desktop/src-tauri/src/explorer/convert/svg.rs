//! SVG to raster, as input only. Local files and the network are never read:
//! only images embedded as `data:` URIs are drawn.

use std::sync::{Arc, OnceLock};

use image::RgbaImage;
use resvg::{tiny_skia, usvg};

/// In automatic mode, small SVGs are drawn at least this large on their longest side.
const AUTO_MIN_SIDE: f32 = 1024.0;
/// Keeps the raster under the pixel limit (8192 x 8192 is about 67 MP).
const MAX_SIDE: f32 = 8192.0;
/// Largest explicit scale (6400 %).
const MAX_SCALE: f32 = 64.0;

pub fn is_svg(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let text = head.trim_start_matches('\u{feff}').trim_start();
    let starts_like_xml = text.starts_with("<?xml")
        || text.starts_with("<svg")
        || text.starts_with("<!--")
        || text.starts_with("<!DOCTYPE svg");
    starts_like_xml && text.contains("<svg")
}

/// Loaded once: reading the system fonts takes a noticeable moment.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

fn options() -> usvg::Options<'static> {
    usvg::Options {
        fontdb: fonts(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn parse(bytes: &[u8]) -> Result<usvg::Tree, String> {
    usvg::Tree::from_data(bytes, &options()).map_err(|_| "Unsupported or damaged image".to_string())
}

/// Raster size for an SVG of the given intrinsic size. `Some(scale)` multiplies the
/// intrinsic size (1.0 is 100 %); `None` picks a scale that makes the longest side at
/// least `AUTO_MIN_SIDE`. Either way the result stays under `MAX_SIDE`.
pub fn render_size(width: f32, height: f32, scale: Option<f32>) -> (u32, u32) {
    let longest = width.max(height).max(1.0);
    let mut factor = match scale {
        Some(scale) => scale.clamp(0.01, MAX_SCALE),
        None if longest < AUTO_MIN_SIDE => AUTO_MIN_SIDE / longest,
        None => 1.0,
    };
    if longest * factor > MAX_SIDE {
        factor = MAX_SIDE / longest;
    }
    (
        ((width * factor).round() as u32).max(1),
        ((height * factor).round() as u32).max(1),
    )
}

/// The size the SVG would be rendered at, without rendering it.
pub fn size(bytes: &[u8], scale: Option<f32>) -> Result<(u32, u32), String> {
    let tree = parse(bytes)?;
    let size = tree.size();
    Ok(render_size(size.width(), size.height(), scale))
}

pub fn render(bytes: &[u8], scale: Option<f32>) -> Result<RgbaImage, String> {
    let tree = parse(bytes)?;
    let size = tree.size();
    let (width, height) = render_size(size.width(), size.height(), scale);
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("Image too large")?;
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / size.width(),
        height as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    // tiny-skia stores premultiplied alpha; the rest of the pipeline expects straight alpha.
    let mut raw = Vec::with_capacity(width as usize * height as usize * 4);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        raw.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    RgbaImage::from_raw(width, height, raw).ok_or_else(|| "Image too large".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED_RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><rect width="64" height="32" fill="#ff0000"/></svg>"##;

    #[test]
    fn detects_svg_by_content() {
        assert!(is_svg(RED_RECT.as_bytes()));
        assert!(is_svg(format!("<?xml version=\"1.0\"?>\n{RED_RECT}").as_bytes()));
        assert!(is_svg(format!("\u{feff}  {RED_RECT}").as_bytes()));
        assert!(!is_svg(b"\x89PNG\r\n\x1a\n"));
        assert!(!is_svg(b"<html><body>hello</body></html>"));
        assert!(!is_svg(b""));
    }

    #[test]
    fn one_hundred_percent_is_the_intrinsic_size() {
        assert_eq!(render_size(100.0, 50.0, Some(1.0)), (100, 50));
        assert_eq!(render_size(64.0, 32.0, Some(1.0)), (64, 32));
    }

    #[test]
    fn explicit_scales_multiply_the_intrinsic_size() {
        assert_eq!(render_size(100.0, 50.0, Some(2.0)), (200, 100));
        assert_eq!(render_size(24.0, 24.0, Some(4.0)), (96, 96));
        assert_eq!(render_size(100.0, 50.0, Some(0.5)), (50, 25));
    }

    #[test]
    fn automatic_scale_makes_small_svgs_sharp() {
        assert_eq!(render_size(100.0, 50.0, None), (1024, 512));
        assert_eq!(render_size(2000.0, 1000.0, None), (2000, 1000));
    }

    #[test]
    fn huge_results_are_capped() {
        assert_eq!(render_size(20000.0, 10000.0, None), (8192, 4096));
        assert_eq!(render_size(1000.0, 500.0, Some(64.0)), (8192, 4096));
    }

    #[test]
    fn renders_at_the_computed_size_with_the_right_colors() {
        assert_eq!(size(RED_RECT.as_bytes(), Some(8.0)).unwrap(), (512, 256));
        let image = render(RED_RECT.as_bytes(), Some(8.0)).unwrap();
        assert_eq!((image.width(), image.height()), (512, 256));
        assert_eq!(image.get_pixel(256, 128).0, [255, 0, 0, 255]);
    }

    #[test]
    fn a_larger_scale_is_drawn_sharp_not_stretched() {
        // A 1 px stripe stays a crisp 4 px stripe at 400 %, instead of being blurred.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect x="2" width="1" height="8" fill="#000000"/></svg>"##;
        let image = render(svg.as_bytes(), Some(4.0)).unwrap();
        assert_eq!((image.width(), image.height()), (32, 32));
        assert_eq!(image.get_pixel(7, 16).0[3], 0);
        assert_eq!(image.get_pixel(8, 16).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(11, 16).0, [0, 0, 0, 255]);
        assert_eq!(image.get_pixel(12, 16).0[3], 0);
    }

    #[test]
    fn keeps_straight_alpha() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><rect width="32" height="32" fill="#ff0000" fill-opacity="0.5"/></svg>"##;
        let image = render(svg.as_bytes(), Some(8.0)).unwrap();
        let [r, g, b, a] = image.get_pixel(100, 128).0;
        assert!(r >= 250 && g == 0 && b == 0, "{r} {g} {b}");
        assert!((120..=135).contains(&a), "{a}");
        assert_eq!(image.get_pixel(400, 128).0[3], 0);
    }

    #[test]
    fn never_reads_local_files_referenced_by_the_svg() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.png");
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 8, image::Rgba([0, 255, 0, 255])))
            .save(&secret)
            .unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="64" height="32"><image xlink:href="{}" width="64" height="32"/></svg>"#,
            secret.display()
        );
        let image = render(svg.as_bytes(), Some(8.0)).unwrap();
        assert!(image.pixels().all(|pixel| pixel.0[3] == 0));
    }

    #[test]
    fn garbage_is_a_clean_error() {
        assert_eq!(
            render(b"<svg this is not valid", None).unwrap_err(),
            "Unsupported or damaged image"
        );
    }
}
