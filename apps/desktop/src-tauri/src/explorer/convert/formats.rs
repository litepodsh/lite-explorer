use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
    Bmp,
    Tiff,
    Ico,
    Avif,
}

/// Extensions the converter can open, lowercase and without the dot.
pub const READABLE_EXTENSIONS: [&str; 13] = [
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "ico", "svg", "heic", "heif", "avif",
];

impl ImageFormat {
    pub const WRITABLE: [ImageFormat; 8] = [
        ImageFormat::Png,
        ImageFormat::Jpeg,
        ImageFormat::Webp,
        ImageFormat::Gif,
        ImageFormat::Bmp,
        ImageFormat::Tiff,
        ImageFormat::Ico,
        ImageFormat::Avif,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Webp => "webp",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Ico => "ico",
            ImageFormat::Avif => "avif",
        }
    }

    pub fn from_extension(extension: &str) -> Option<ImageFormat> {
        match extension.to_ascii_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "webp" => Some(ImageFormat::Webp),
            "gif" => Some(ImageFormat::Gif),
            "bmp" => Some(ImageFormat::Bmp),
            "tif" | "tiff" => Some(ImageFormat::Tiff),
            "ico" => Some(ImageFormat::Ico),
            "avif" => Some(ImageFormat::Avif),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<ImageFormat> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(ImageFormat::from_extension)
    }

    /// Formats where the quality setting changes the output.
    #[allow(dead_code)] // Used by the Convert dialog (next plan).
    pub fn is_lossy(self) -> bool {
        matches!(self, ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Avif)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_extensions_case_insensitively() {
        assert_eq!(ImageFormat::from_extension("JPG"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_extension("TIF"), Some(ImageFormat::Tiff));
        assert_eq!(ImageFormat::from_extension("txt"), None);
    }

    #[test]
    fn detects_format_from_path() {
        assert_eq!(
            ImageFormat::from_path(Path::new("/a/b/photo.PNG")),
            Some(ImageFormat::Png)
        );
        assert_eq!(ImageFormat::from_path(Path::new("/a/b/noext")), None);
    }

    #[test]
    fn only_jpeg_webp_and_avif_are_lossy() {
        let lossy: Vec<_> = ImageFormat::WRITABLE
            .into_iter()
            .filter(|format| format.is_lossy())
            .collect();
        assert_eq!(
            lossy,
            vec![ImageFormat::Jpeg, ImageFormat::Webp, ImageFormat::Avif]
        );
    }

    #[test]
    fn every_readable_extension_maps_to_a_format() {
        // SVG, HEIC and HEIF are read-only input: there is no `ImageFormat` to write.
        let input_only = ["svg", "heic", "heif"];
        for extension in READABLE_EXTENSIONS {
            let known = ImageFormat::from_extension(extension).is_some();
            assert_eq!(known, !input_only.contains(&extension), "{extension}");
        }
    }

    #[test]
    fn avif_is_writable() {
        assert_eq!(ImageFormat::from_extension("AVIF"), Some(ImageFormat::Avif));
        assert!(ImageFormat::WRITABLE.contains(&ImageFormat::Avif));
    }
}
