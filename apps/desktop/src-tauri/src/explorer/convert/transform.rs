use image::{imageops::FilterType, DynamicImage};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Resize {
    None,
    /// 1 to 100. Values above 100 are clamped: images are never enlarged.
    Percent { value: u32 },
    /// Fits inside the box, keeping the aspect ratio. A side of 0 means unbounded.
    Fit { width: u32, height: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transform {
    /// EXIF orientation, 1 to 8. Anything else is treated as 1.
    pub orientation: u8,
    /// Clockwise degrees: 0, 90, 180 or 270.
    pub rotate: u16,
    pub flip_h: bool,
    pub flip_v: bool,
    pub resize: Resize,
}

pub fn target_size(width: u32, height: u32, resize: &Resize) -> (u32, u32) {
    match *resize {
        Resize::None => (width, height),
        Resize::Percent { value } => {
            let percent = value.clamp(1, 100) as f64 / 100.0;
            (scale(width, percent), scale(height, percent))
        }
        Resize::Fit {
            width: box_width,
            height: box_height,
        } => {
            let mut ratio = 1.0_f64;
            if box_width > 0 {
                ratio = ratio.min(box_width as f64 / width as f64);
            }
            if box_height > 0 {
                ratio = ratio.min(box_height as f64 / height as f64);
            }
            (scale(width, ratio), scale(height, ratio))
        }
    }
}

fn scale(side: u32, ratio: f64) -> u32 {
    ((side as f64 * ratio).round() as u32).max(1)
}

fn apply_orientation(image: DynamicImage, orientation: u8) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate270().fliph(),
        8 => image.rotate270(),
        _ => image,
    }
}

/// Order: EXIF orientation, user rotation, flips, then resize.
pub fn apply(image: DynamicImage, transform: &Transform) -> DynamicImage {
    let mut image = apply_orientation(image, transform.orientation);
    image = match transform.rotate % 360 {
        90 => image.rotate90(),
        180 => image.rotate180(),
        270 => image.rotate270(),
        _ => image,
    };
    if transform.flip_h {
        image = image.fliph();
    }
    if transform.flip_v {
        image = image.flipv();
    }
    let (width, height) = target_size(image.width(), image.height(), &transform.resize);
    if (width, height) != (image.width(), image.height()) {
        image = image.resize_exact(width, height, FilterType::Lanczos3);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn blank(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
    }

    fn plain(resize: Resize) -> Transform {
        Transform {
            orientation: 1,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            resize,
        }
    }

    #[test]
    fn percent_halves_both_sides() {
        assert_eq!(
            target_size(4000, 3000, &Resize::Percent { value: 50 }),
            (2000, 1500)
        );
    }

    #[test]
    fn percent_above_100_never_enlarges() {
        assert_eq!(
            target_size(400, 300, &Resize::Percent { value: 250 }),
            (400, 300)
        );
    }

    #[test]
    fn fit_keeps_aspect_ratio() {
        let resize = Resize::Fit { width: 1080, height: 1080 };
        assert_eq!(target_size(4000, 3000, &resize), (1080, 810));
    }

    #[test]
    fn fit_never_enlarges() {
        let resize = Resize::Fit { width: 1000, height: 1000 };
        assert_eq!(target_size(100, 50, &resize), (100, 50));
    }

    #[test]
    fn fit_with_one_unbounded_side() {
        let resize = Resize::Fit { width: 500, height: 0 };
        assert_eq!(target_size(1000, 400, &resize), (500, 200));
    }

    #[test]
    fn tiny_results_never_reach_zero() {
        let resize = Resize::Fit { width: 1, height: 1 };
        assert_eq!(target_size(1000, 10, &resize), (1, 1));
    }

    #[test]
    fn orientation_6_swaps_dimensions() {
        let mut transform = plain(Resize::None);
        transform.orientation = 6;
        let out = apply(blank(20, 10), &transform);
        assert_eq!((out.width(), out.height()), (10, 20));
    }

    #[test]
    fn orientation_2_keeps_dimensions() {
        let mut transform = plain(Resize::None);
        transform.orientation = 2;
        let out = apply(blank(20, 10), &transform);
        assert_eq!((out.width(), out.height()), (20, 10));
    }

    #[test]
    fn rotate_90_swaps_and_flip_keeps() {
        let mut transform = plain(Resize::None);
        transform.rotate = 90;
        transform.flip_h = true;
        let out = apply(blank(20, 10), &transform);
        assert_eq!((out.width(), out.height()), (10, 20));
    }

    #[test]
    fn rotate_then_resize_uses_the_rotated_size() {
        let mut transform = plain(Resize::Fit { width: 10, height: 10 });
        transform.rotate = 90;
        let out = apply(blank(40, 20), &transform);
        assert_eq!((out.width(), out.height()), (5, 10));
    }
}
