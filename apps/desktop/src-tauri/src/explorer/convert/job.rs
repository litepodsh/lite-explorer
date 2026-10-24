use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    decode::decode_file,
    encode::encode,
    formats::ImageFormat,
    metadata::apply as apply_metadata,
    transform::{apply, Resize, Transform},
};

/// Estimates above this many pixels are measured on a reduced copy and scaled up.
const ESTIMATE_PIXELS: u64 = 4_000_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    /// `None` keeps each file's own format.
    pub format: Option<ImageFormat>,
    pub quality: u8,
    pub resize: Resize,
    pub rotate: u16,
    pub flip_h: bool,
    pub flip_v: bool,
    /// When false, EXIF and ICC are copied to JPEG, PNG and WebP outputs.
    pub strip_metadata: bool,
    /// Added to the file stem, for example `" (optimized)"`.
    pub suffix: Option<String>,
    /// Skip files whose result is not smaller than the original.
    pub only_if_smaller: bool,
    /// Scale for SVG sources: `Some(1.0)` is 100 % of their own size, `None` is automatic
    /// (sharp, at least 1024 px on the longest side). Ignored for raster sources.
    #[serde(default)]
    pub vector_scale: Option<f32>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Converted {
    pub source: String,
    pub destination: String,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub name: String,
    pub error: String,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub converted: Vec<Converted>,
    pub failed: Vec<Failure>,
    pub skipped: Vec<String>,
    pub cancelled: bool,
}

fn resolve_format(requested: Option<ImageFormat>, source: &Path) -> Result<ImageFormat, String> {
    requested
        .or_else(|| ImageFormat::from_path(source))
        .ok_or_else(|| "Unsupported format".to_string())
}

fn transform_of(options: &ConvertOptions, orientation: u8) -> Transform {
    Transform {
        orientation,
        rotate: options.rotate,
        flip_h: options.flip_h,
        flip_v: options.flip_v,
        resize: options.resize,
    }
}

/// EXIF and ICC are copied only into formats that can carry them.
fn keeps_metadata(options: &ConvertOptions, format: ImageFormat) -> bool {
    !options.strip_metadata
        && matches!(format, ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Webp)
}

/// `stem + suffix + ext`, then `stem + suffix + " 2" + ext`, and so on, until free.
fn unique_path(parent: &Path, source: &Path, suffix: &str, format: ImageFormat) -> PathBuf {
    let stem = source
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".into());
    let base = format!("{stem}{suffix}");
    let extension = format.extension();
    let first = parent.join(format!("{base}.{extension}"));
    if !first.exists() {
        return first;
    }
    (2..)
        .map(|n| parent.join(format!("{base} {n}.{extension}")))
        .find(|candidate| !candidate.exists())
        .expect("an unused name exists")
}

fn write_atomic(parent: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    temp.write_all(bytes).map_err(|error| error.to_string())?;
    temp.persist_noclobber(target)
        .map_err(|error| error.error.to_string())?;
    Ok(())
}

fn convert_one(
    source: &Path,
    options: &ConvertOptions,
    replace_original: bool,
    remove_original: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<Option<PathBuf>, String> {
    let format = resolve_format(options.format, source)?;
    let bytes = render_file(source, options)?;
    if options.only_if_smaller {
        let original = fs::metadata(source)
            .map_err(|error| error.to_string())?
            .len();
        if bytes.len() as u64 >= original {
            return Ok(None);
        }
    }
    let parent = source.parent().ok_or("Invalid local path")?;
    let suffix = options.suffix.as_deref().unwrap_or("");
    let mut target = unique_path(parent, source, suffix, format);
    write_atomic(parent, &target, &bytes)?;
    if replace_original {
        if let Err(error) = decode_file(&target, None) {
            let _ = fs::remove_file(&target);
            return Err(format!("Couldn’t verify the converted file: {error}"));
        }
        remove_original(source)?;
        // The original name is free now: take it instead of "name 2".
        let preferred = parent.join(format!(
            "{}{}.{}",
            source
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default(),
            suffix,
            format.extension()
        ));
        if preferred != target && !preferred.exists() && fs::rename(&target, &preferred).is_ok() {
            target = preferred;
        }
    }
    Ok(Some(target))
}

pub fn run_batch(
    paths: &[PathBuf],
    options: &ConvertOptions,
    replace_original: bool,
    cancelled: &dyn Fn() -> bool,
    remove_original: &dyn Fn(&Path) -> Result<(), String>,
    progress: &mut dyn FnMut(u64, u64),
) -> Outcome {
    let mut outcome = Outcome::default();
    let total = paths.len() as u64;
    for (index, path) in paths.iter().enumerate() {
        if cancelled() {
            outcome.cancelled = true;
            break;
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match convert_one(path, options, replace_original, remove_original) {
            Ok(Some(destination)) => outcome.converted.push(Converted {
                source: path.to_string_lossy().into_owned(),
                destination: destination.to_string_lossy().into_owned(),
            }),
            Ok(None) => outcome.skipped.push(name),
            Err(error) => outcome.failed.push(Failure { name, error }),
        }
        progress(index as u64 + 1, total);
    }
    outcome
}

/// The exact bytes a conversion would write, without touching the disk.
pub fn render_file(path: &Path, options: &ConvertOptions) -> Result<Vec<u8>, String> {
    let format = resolve_format(options.format, path)?;
    let decoded = decode_file(path, options.vector_scale)?;
    let image = apply(decoded.image, &transform_of(options, decoded.orientation));
    let mut bytes = encode(&image, format, options.quality)?;
    if keeps_metadata(options, format) {
        bytes = apply_metadata(&decoded.metadata, bytes);
    }
    Ok(bytes)
}

/// Size the output would have. Big images are measured on a ~4 MP copy and scaled.
pub fn estimate_size(path: &Path, options: &ConvertOptions) -> Result<u64, String> {
    let format = resolve_format(options.format, path)?;
    let decoded = decode_file(path, options.vector_scale)?;
    let image = apply(decoded.image, &transform_of(options, decoded.orientation));
    let pixels = image.width() as u64 * image.height() as u64;
    if pixels <= ESTIMATE_PIXELS {
        let mut bytes = encode(&image, format, options.quality)?;
        if keeps_metadata(options, format) {
            bytes = apply_metadata(&decoded.metadata, bytes);
        }
        return Ok(bytes.len() as u64);
    }
    let scale = (ESTIMATE_PIXELS as f64 / pixels as f64).sqrt();
    let small = image.resize_exact(
        ((image.width() as f64 * scale).round() as u32).max(1),
        ((image.height() as f64 * scale).round() as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    let small_pixels = small.width() as f64 * small.height() as f64;
    let small_bytes = encode(&small, format, options.quality)?.len() as f64;
    Ok((small_bytes * pixels as f64 / small_pixels) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat as Out, RgbaImage};
    use std::{cell::RefCell, fs};

    fn options(format: Option<ImageFormat>) -> ConvertOptions {
        ConvertOptions {
            format,
            quality: 80,
            resize: Resize::None,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            strip_metadata: false,
            suffix: None,
            only_if_smaller: false,
            vector_scale: None,
        }
    }

    fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = dir.join(name);
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
            .save_with_format(&path, Out::Png)
            .unwrap();
        path
    }

    fn run(paths: &[PathBuf], options: &ConvertOptions, replace: bool) -> Outcome {
        run_batch(
            paths,
            options,
            replace,
            &|| false,
            &|path| fs::remove_file(path).map_err(|e| e.to_string()),
            &mut |_, _| {},
        )
    }

    #[test]
    fn converts_next_to_the_original_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let outcome = run(std::slice::from_ref(&source), &options(Some(ImageFormat::Jpeg)), false);
        assert_eq!(outcome.converted.len(), 1);
        assert!(source.exists());
        assert!(dir.path().join("a.jpg").exists());
    }

    #[test]
    fn second_run_uses_keep_both_naming() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let opts = options(Some(ImageFormat::Jpeg));
        run(std::slice::from_ref(&source), &opts, false);
        run(&[source], &opts, false);
        assert!(dir.path().join("a 2.jpg").exists());
    }

    #[test]
    fn same_format_never_overwrites_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let before = fs::read(&source).unwrap();
        run(std::slice::from_ref(&source), &options(Some(ImageFormat::Png)), false);
        assert_eq!(fs::read(&source).unwrap(), before);
        assert!(dir.path().join("a 2.png").exists());
    }

    #[test]
    fn keeps_each_files_own_format_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let mut opts = options(None);
        opts.suffix = Some(" (optimized)".into());
        run(&[source], &opts, false);
        assert!(dir.path().join("a (optimized).png").exists());
    }

    #[test]
    fn replace_original_removes_source_and_reuses_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let removed = RefCell::new(Vec::new());
        let outcome = run_batch(
            std::slice::from_ref(&source),
            &options(Some(ImageFormat::Png)),
            true,
            &|| false,
            &|path| {
                removed.borrow_mut().push(path.to_path_buf());
                fs::remove_file(path).map_err(|e| e.to_string())
            },
            &mut |_, _| {},
        );
        assert_eq!(removed.borrow().as_slice(), std::slice::from_ref(&source));
        assert_eq!(outcome.converted[0].destination, source.to_string_lossy());
        assert!(source.exists());
    }

    #[test]
    fn one_bad_file_does_not_stop_the_batch() {
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.png");
        fs::write(&bad, b"not an image").unwrap();
        let good = write_png(dir.path(), "good.png", 8, 8);
        let outcome = run(&[bad, good], &options(Some(ImageFormat::Jpeg)), false);
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].name, "bad.png");
        assert_eq!(outcome.converted.len(), 1);
    }

    #[test]
    fn cancel_stops_before_touching_anything() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 8, 8);
        let outcome = run_batch(
            &[source],
            &options(Some(ImageFormat::Jpeg)),
            false,
            &|| true,
            &|_| Ok(()),
            &mut |_, _| {},
        );
        assert!(outcome.cancelled);
        assert!(outcome.converted.is_empty());
        assert!(!dir.path().join("a.jpg").exists());
    }

    #[test]
    fn reports_progress_per_image() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_png(dir.path(), "a.png", 4, 4);
        let b = write_png(dir.path(), "b.png", 4, 4);
        let seen = RefCell::new(Vec::new());
        run_batch(
            &[a, b],
            &options(Some(ImageFormat::Jpeg)),
            false,
            &|| false,
            &|_| Ok(()),
            &mut |done, total| seen.borrow_mut().push((done, total)),
        );
        assert_eq!(seen.into_inner(), vec![(1, 2), (2, 2)]);
    }

    #[test]
    fn estimate_matches_the_real_output_size() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 64, 64);
        let opts = options(Some(ImageFormat::Jpeg));
        let estimate = estimate_size(&source, &opts).unwrap();
        run(&[source], &opts, false);
        let real = fs::metadata(dir.path().join("a.jpg")).unwrap().len();
        assert_eq!(estimate, real);
    }

    #[test]
    fn resize_changes_the_output_dimensions() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_png(dir.path(), "a.png", 100, 50);
        let mut opts = options(Some(ImageFormat::Png));
        opts.resize = Resize::Percent { value: 50 };
        run(&[source], &opts, false);
        let out = image::open(dir.path().join("a 2.png")).unwrap();
        assert_eq!((out.width(), out.height()), (50, 25));
    }

    fn write_jpeg_with_exif(dir: &Path, name: &str) -> PathBuf {
        use crate::explorer::convert::{
            encode::encode,
            metadata::{exif_orientation_le, with_exif},
        };
        let jpeg = encode(
            &DynamicImage::ImageRgba8(RgbaImage::new(8, 6)),
            ImageFormat::Jpeg,
            90,
        )
        .unwrap();
        let path = dir.join(name);
        fs::write(&path, with_exif(jpeg, &exif_orientation_le(6))).unwrap();
        path
    }

    fn exif_orientation(path: &Path) -> Option<u32> {
        let bytes = fs::read(path).unwrap();
        exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(bytes))
            .ok()?
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
            .value
            .get_uint(0)
    }

    #[test]
    fn keeps_exif_with_orientation_reset_when_not_stripping() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_jpeg_with_exif(dir.path(), "p.jpg");
        let mut opts = options(Some(ImageFormat::Jpeg));
        opts.strip_metadata = false;
        run(&[source], &opts, false);
        assert_eq!(exif_orientation(&dir.path().join("p 2.jpg")), Some(1));
    }

    #[test]
    fn strips_exif_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_jpeg_with_exif(dir.path(), "p.jpg");
        let mut opts = options(Some(ImageFormat::Jpeg));
        opts.strip_metadata = true;
        run(&[source], &opts, false);
        assert_eq!(exif_orientation(&dir.path().join("p 2.jpg")), None);
    }

    #[test]
    fn estimate_includes_kept_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_jpeg_with_exif(dir.path(), "p.jpg");
        let opts = options(Some(ImageFormat::Jpeg));
        let estimate = estimate_size(&source, &opts).unwrap();
        run(&[source], &opts, false);
        let real = fs::metadata(dir.path().join("p 2.jpg")).unwrap().len();
        assert_eq!(estimate, real);
    }

    #[test]
    fn render_file_returns_exactly_what_convert_writes() {
        let dir = tempfile::tempdir().unwrap();
        let source = write_jpeg_with_exif(dir.path(), "p.jpg");
        let mut opts = options(Some(ImageFormat::Webp));
        opts.quality = 40;
        opts.resize = Resize::Percent { value: 50 };
        let rendered = render_file(&source, &opts).unwrap();
        run(&[source], &opts, false);
        assert_eq!(rendered, fs::read(dir.path().join("p.webp")).unwrap());
    }

    #[test]
    fn svg_scale_sets_the_output_size() {
        let dir = tempfile::tempdir().unwrap();
        let svg = dir.path().join("logo.svg");
        fs::write(
            &svg,
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><rect width="64" height="32" fill="#336699"/></svg>"##,
        )
        .unwrap();
        let mut opts = options(Some(ImageFormat::Png));
        opts.vector_scale = Some(1.0);
        run(std::slice::from_ref(&svg), &opts, false);
        assert_eq!(image::image_dimensions(dir.path().join("logo.png")).unwrap(), (64, 32));
        opts.vector_scale = Some(4.0);
        run(std::slice::from_ref(&svg), &opts, false);
        assert_eq!(image::image_dimensions(dir.path().join("logo 2.png")).unwrap(), (256, 128));
        opts.vector_scale = None;
        run(&[svg], &opts, false);
        assert_eq!(image::image_dimensions(dir.path().join("logo 3.png")).unwrap(), (1024, 512));
    }
}
