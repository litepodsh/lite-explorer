use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        OnceLock,
    },
    time::UNIX_EPOCH,
};

use image::{ImageFormat, ImageReader};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use tokio::{sync::Semaphore, task::JoinSet};

use super::{register_local, MediaRegistry};
use crate::explorer::{
    convert::{
        decode::decode_file,
        transform::{apply, Resize, Transform},
    },
    transcode::{
        manifest,
        tools::{locate, system_dirs, Tools},
    },
};

const EDGE: u32 = 256;
const MAX_SOURCE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const VIDEO_EXTENSIONS: [&str; 12] = [
    "mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "m2ts", "ogv",
];

/// Monotonically increasing generation ID used to drop stale thumbnail work when the user
/// switches folders or scrolls rapidly.
static THUMBNAIL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Global semaphore bounding concurrent heavy thumbnail decodes across all IPC calls so
/// cancelled/superseded requests never starve a newly opened folder.
fn decode_semaphore() -> &'static Semaphore {
    static SEMAPHORE: OnceLock<Semaphore> = OnceLock::new();
    SEMAPHORE.get_or_init(|| Semaphore::new(4))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageThumbnail {
    pub path: String,
    pub url: String,
}

fn modified_ms(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |time| time.as_millis())
}

pub fn cache_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mut hash = Sha256::new();
    hash.update(b"liteexplorer-thumbnail-v1\0");
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified_ms(metadata).to_le_bytes());
    hash.update(EDGE.to_le_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn trim_cache(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata
                .is_file()
                .then(|| (entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect::<Vec<_>>();
    let mut total = files.iter().map(|(_, size, _)| size).sum::<u64>();
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in files {
        if total <= MAX_CACHE_BYTES {
            break;
        }
        if fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// Formats the web view cannot display (HEIC, AVIF). They are decoded in Rust and shown
/// as a cached PNG, in the viewer and in thumbnails.
pub fn needs_decode(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        ["heic", "heif", "avif"]
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

pub fn is_video(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        VIDEO_EXTENSIONS
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

#[cfg(target_os = "macos")]
fn native_video_thumbnail(source: &Path, target: &Path) -> bool {
    use objc2::{
        class,
        encode::{Encode, Encoding, RefEncode},
        msg_send,
        rc::autoreleasepool,
        runtime::{AnyObject, Bool},
    };
    use std::ffi::{c_void, CString};

    #[repr(C)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CMTime {
        value: i64,
        timescale: i32,
        flags: u32,
        epoch: i64,
    }
    unsafe impl Encode for CMTime {
        const ENCODING: Encoding = Encoding::Struct(
            "?",
            &[i64::ENCODING, i32::ENCODING, u32::ENCODING, i64::ENCODING],
        );
    }
    unsafe impl RefEncode for CMTime {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }

    #[repr(C)]
    struct CGImage {
        _private: [u8; 0],
    }
    unsafe impl RefEncode for CGImage {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGImage", &[]));
    }

    extern "C" {
        fn CGImageRelease(image: *mut CGImage);
    }

    let Some(c_path) = CString::new(source.to_string_lossy().as_bytes()).ok() else {
        return false;
    };

    let bytes = autoreleasepool(|_| unsafe {
        let ns_path: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
        if ns_path.is_null() {
            return None;
        }
        let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: ns_path];
        if url.is_null() {
            return None;
        }
        let asset: *mut AnyObject = msg_send![class!(AVURLAsset), assetWithURL: url];
        if asset.is_null() {
            return None;
        }
        let generator: *mut AnyObject =
            msg_send![class!(AVAssetImageGenerator), assetImageGeneratorWithAsset: asset];
        if generator.is_null() {
            return None;
        }
        let _: () = msg_send![generator, setAppliesPreferredTrackTransform: Bool::YES];
        let _: () = msg_send![
            generator,
            setMaximumSize: CGSize {
                width: EDGE as f64,
                height: EDGE as f64,
            }
        ];
        // kCMTimeFlags_Valid (1) | kCMTimeFlags_PositiveInfinity (4) = 5: picks the nearest keyframe without decoding intermediate frames.
        let infinity = CMTime {
            value: 0,
            timescale: 0,
            flags: 5,
            epoch: 0,
        };
        let _: () = msg_send![generator, setRequestedTimeToleranceBefore: infinity];
        let _: () = msg_send![generator, setRequestedTimeToleranceAfter: infinity];

        let at_one_sec = CMTime {
            value: 1,
            timescale: 1,
            flags: 1,
            epoch: 0,
        };
        let mut actual = CMTime {
            value: 0,
            timescale: 0,
            flags: 0,
            epoch: 0,
        };
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let mut cg_image: *mut CGImage = msg_send![
            generator,
            copyCGImageAtTime: at_one_sec,
            actualTime: &mut actual,
            error: &mut error
        ];
        if cg_image.is_null() {
            let at_start = CMTime {
                value: 0,
                timescale: 1,
                flags: 1,
                epoch: 0,
            };
            error = std::ptr::null_mut();
            cg_image = msg_send![
                generator,
                copyCGImageAtTime: at_start,
                actualTime: &mut actual,
                error: &mut error
            ];
        }
        if cg_image.is_null() {
            return None;
        }

        let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut AnyObject = msg_send![rep, initWithCGImage: cg_image];
        CGImageRelease(cg_image);
        if rep.is_null() {
            return None;
        }
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let data: *mut AnyObject =
            msg_send![rep, representationUsingType: 4usize, properties: properties];
        let out = if data.is_null() {
            None
        } else {
            let ptr: *const c_void = msg_send![data, bytes];
            let len: usize = msg_send![data, length];
            Some(std::slice::from_raw_parts(ptr as *const u8, len).to_vec())
        };
        let _: () = msg_send![rep, release];
        out
    });

    let Some(bytes) = bytes else {
        return false;
    };
    let temporary = target.with_extension("tmp");
    fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, target).is_ok()
}

#[cfg(target_os = "windows")]
fn native_video_thumbnail(source: &Path, target: &Path) -> bool {
    use std::{ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::core::GUID;
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP,
    };
    use windows_sys::Win32::UI::Shell::SHCreateItemFromParsingName;

    #[repr(C)]
    struct IShellItemImageFactoryVtbl {
        pub query_interface: unsafe extern "system" fn(*mut std::ffi::c_void, *const GUID, *mut *mut std::ffi::c_void) -> i32,
        pub add_ref: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        pub release: unsafe extern "system" fn(*mut std::ffi::c_void) -> u32,
        pub get_image: unsafe extern "system" fn(*mut std::ffi::c_void, SIZE, i32, *mut HBITMAP) -> i32,
    }

    #[repr(C)]
    struct IShellItemImageFactory {
        pub lp_vtbl: *const IShellItemImageFactoryVtbl,
    }

    // BCC18B79-BA16-442F-80C4-8A59C30C463B
    const IID_ISHELL_ITEM_IMAGE_FACTORY: GUID = GUID {
        data1: 0xBCC18B79,
        data2: 0xBA16,
        data3: 0x442F,
        data4: [0x80, 0xC4, 0x8A, 0x59, 0xC3, 0x0C, 0x46, 0x3B],
    };
    // SIIGBF_BIGGERSIZEOK (0x01) | SIIGBF_THUMBNAILONLY (0x08)
    const FLAGS: i32 = 0x01 | 0x08;

    let wide: Vec<u16> = OsStr::new(source)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut factory: *mut IShellItemImageFactory = ptr::null_mut();
    let hr = unsafe {
        SHCreateItemFromParsingName(
            wide.as_ptr(),
            ptr::null_mut(),
            &IID_ISHELL_ITEM_IMAGE_FACTORY,
            &mut factory as *mut _ as *mut *mut std::ffi::c_void,
        )
    };
    if hr < 0 || factory.is_null() {
        return false;
    }

    let mut hbitmap: HBITMAP = ptr::null_mut();
    let size = SIZE {
        cx: EDGE as i32,
        cy: EDGE as i32,
    };
    let hr = unsafe { ((*(*factory).lp_vtbl).get_image)(factory as *mut _, size, FLAGS, &mut hbitmap) };
    unsafe { ((*(*factory).lp_vtbl).release)(factory as *mut _) };
    if hr < 0 || hbitmap.is_null() {
        return false;
    }

    let mut bitmap = BITMAP::default();
    let got_bitmap = unsafe {
        GetObjectW(
            hbitmap,
            size_of::<BITMAP>() as i32,
            &mut bitmap as *mut _ as *mut _,
        )
    };
    if got_bitmap == 0 || bitmap.bmWidth <= 0 || bitmap.bmHeight <= 0 {
        unsafe { DeleteObject(hbitmap) };
        return false;
    }
    let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width;
    bmi.bmiHeader.biHeight = -height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
    let previous = unsafe { SelectObject(dc, hbitmap) };
    let lines = unsafe {
        GetDIBits(
            dc,
            hbitmap,
            0,
            height as u32,
            buffer.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    unsafe {
        SelectObject(dc, previous);
        DeleteDC(dc);
        DeleteObject(hbitmap);
    }
    if lines == 0 {
        return false;
    }

    let opaque = buffer.chunks_exact(4).all(|pixel| pixel[3] == 0);
    let mut rgba = Vec::with_capacity(buffer.len());
    for pixel in buffer.chunks_exact(4) {
        rgba.push(pixel[2]);
        rgba.push(pixel[1]);
        rgba.push(pixel[0]);
        rgba.push(if opaque { 255 } else { pixel[3] });
    }

    let temporary = target.with_extension("tmp");
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let Ok(mut writer) = encoder.write_header() else {
            return false;
        };
        if writer.write_image_data(&rgba).is_err() {
            return false;
        }
    }
    fs::write(&temporary, bytes).is_ok() && fs::rename(&temporary, target).is_ok()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_video_thumbnail(_source: &Path, _target: &Path) -> bool {
    false
}

fn ffmpeg_video_thumbnail(tools: &Tools, source: &Path, target: &Path) -> bool {
    let temporary = target.with_extension("tmp.png");
    let scale = format!("scale={EDGE}:{EDGE}:force_original_aspect_ratio=decrease:flags=fast_bilinear");
    for seek in ["1.0", "0.0"] {
        let status = Command::new(&tools.ffmpeg)
            .args(["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-ss", seek])
            .arg("-i")
            .arg(source)
            .args(["-frames:v", "1", "-vf", &scale])
            .arg(&temporary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.is_ok_and(|s| s.success()) && temporary.metadata().is_ok_and(|m| m.len() > 0) {
            return fs::rename(&temporary, target).is_ok();
        }
    }
    let _ = fs::remove_file(&temporary);
    false
}

fn make_video_thumbnail(tools: Option<&Tools>, source: &Path, target: &Path) -> bool {
    if native_video_thumbnail(source, target) {
        return true;
    }
    tools.is_some_and(|tools| ffmpeg_video_thumbnail(tools, source, target))
}

/// Decodes `source` at full size and applies its EXIF orientation.
fn decode_upright(source: &Path) -> Result<image::DynamicImage, String> {
    let decoded = decode_file(source, Some(1.0))?;
    Ok(apply(
        decoded.image,
        &Transform {
            orientation: decoded.orientation,
            rotate: 0,
            flip_h: false,
            flip_v: false,
            resize: Resize::None,
        },
    ))
}

fn preview_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mut hash = Sha256::new();
    hash.update(b"liteexplorer-preview-v1\0");
    hash.update(path.to_string_lossy().as_bytes());
    hash.update(metadata.len().to_le_bytes());
    hash.update(modified_ms(metadata).to_le_bytes());
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A full-size PNG of a HEIC or AVIF file, cached by path, size and modified time.
pub fn decoded_png(cache_root: &Path, source: &Path) -> Result<PathBuf, String> {
    let metadata = fs::metadata(source).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
        return Err("File too large to preview".into());
    }
    let dir = cache_root.join("previews");
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let target = dir.join(format!("{}.png", preview_key(source, &metadata)));
    if !target.exists() {
        let image = decode_upright(source)?;
        let temporary = target.with_extension("tmp");
        image
            .save_with_format(&temporary, ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        fs::rename(&temporary, &target).map_err(|error| error.to_string())?;
        trim_cache(&dir);
    }
    Ok(target)
}

/// Checks if a thumbnail already exists on disk (or is an SVG that needs no decoding).
/// Returns:
/// - `Ok(Some((source, target_path)))` when the thumbnail is already cached or SVG.
/// - `Ok(None)` when the file cannot have a thumbnail (not a file, too large, missing).
/// - `Err((source, source_path, target_path, is_video))` when decoding is required.
fn resolve_cached_or_pending(
    cache: &Path,
    source: &str,
) -> Result<Option<(String, PathBuf)>, (String, PathBuf, PathBuf, bool)> {
    let Ok(source_path) = fs::canonicalize(source) else {
        return Ok(None);
    };
    let Ok(metadata) = fs::metadata(&source_path) else {
        return Ok(None);
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    let video = is_video(&source_path);
    if !video && metadata.len() > MAX_SOURCE_BYTES {
        return Ok(None);
    }
    if source_path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
    {
        return Ok(Some((source.to_owned(), source_path)));
    }
    let target = cache.join(format!("{}.png", cache_key(&source_path, &metadata)));
    if target.exists() {
        return Ok(Some((source.to_owned(), target)));
    }
    Err((source.to_owned(), source_path, target, video))
}

fn generate_thumbnail(
    source_path: &Path,
    target: &Path,
    video: bool,
    tools: Option<&Tools>,
) -> bool {
    if target.exists() {
        return true;
    }
    if video {
        make_video_thumbnail(tools, source_path, target)
    } else {
        let Some(image) = (if needs_decode(source_path) {
            decode_upright(source_path).ok()
        } else {
            ImageReader::open(source_path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
        }) else {
            return false;
        };
        let thumbnail = image.thumbnail(EDGE, EDGE);
        let temporary = target.with_extension("tmp");
        thumbnail
            .save_with_format(&temporary, ImageFormat::Png)
            .is_ok()
            && fs::rename(temporary, target).is_ok()
    }
}

#[cfg(test)]
fn make_thumbnail(cache: &Path, source: &str, tools: Option<&Tools>) -> Option<(String, PathBuf)> {
    match resolve_cached_or_pending(cache, source) {
        Ok(cached) => cached,
        Err((source, source_path, target, video)) => {
            generate_thumbnail(&source_path, &target, video, tools).then_some((source, target))
        }
    }
}

#[tauri::command]
pub fn cancel_thumbnails(generation: u64) {
    THUMBNAIL_GENERATION.fetch_max(generation, Ordering::SeqCst);
}

#[tauri::command]
pub async fn image_thumbnails(
    app: AppHandle,
    registry: State<'_, MediaRegistry>,
    paths: Vec<String>,
    generation: Option<u64>,
    cached_only: Option<bool>,
) -> Result<Vec<ImageThumbnail>, String> {
    if let Some(gen) = generation {
        let current = THUMBNAIL_GENERATION.fetch_max(gen, Ordering::SeqCst).max(gen);
        if gen < current {
            return Ok(Vec::new());
        }
    }
    let is_stale = || {
        generation.is_some_and(|gen| THUMBNAIL_GENERATION.load(Ordering::Relaxed) > gen)
    };
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("thumbnails");
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;

    // Fast path: check disk cache first without acquiring decode semaphore permits.
    let cache_for_check = cache.clone();
    let (mut ready, to_generate) = tauri::async_runtime::spawn_blocking(move || {
        let mut ready = Vec::new();
        let mut to_generate = Vec::new();
        for path in paths {
            if generation.is_some_and(|gen| THUMBNAIL_GENERATION.load(Ordering::Relaxed) > gen) {
                break;
            }
            match resolve_cached_or_pending(&cache_for_check, &path) {
                Ok(Some(hit)) => ready.push(hit),
                Ok(None) => {}
                Err(miss) => to_generate.push(miss),
            }
        }
        (ready, to_generate)
    })
    .await
    .map_err(|error| error.to_string())?;

    if !cached_only.unwrap_or(false) && !to_generate.is_empty() && !is_stale() {
        let managed = app
            .path()
            .app_data_dir()
            .ok()
            .map(|dir| dir.join("ffmpeg").join(manifest::VERSION));
        let has_video = to_generate.iter().any(|(_, _, _, video)| *video);
        let tools = if has_video {
            std::sync::Arc::new(locate(managed.as_deref(), &system_dirs()))
        } else {
            std::sync::Arc::new(None)
        };
        let semaphore = decode_semaphore();
        let mut jobs = JoinSet::new();
        for (source, source_path, target, video) in to_generate {
            if is_stale() {
                break;
            }
            let permit = semaphore
                .acquire()
                .await
                .map_err(|error| error.to_string())?;
            if is_stale() {
                drop(permit);
                break;
            }
            let tools = tools.clone();
            jobs.spawn(async move {
                let _permit = permit;
                if generation.is_some_and(|gen| THUMBNAIL_GENERATION.load(Ordering::Relaxed) > gen)
                {
                    return None;
                }
                tauri::async_runtime::spawn_blocking(move || {
                    if generation
                        .is_some_and(|gen| THUMBNAIL_GENERATION.load(Ordering::Relaxed) > gen)
                    {
                        return None;
                    }
                    generate_thumbnail(&source_path, &target, video, tools.as_ref().as_ref())
                        .then_some((source, target))
                })
                .await
                .ok()
                .flatten()
            });
        }
        let mut wrote_new = false;
        while let Some(result) = jobs.join_next().await {
            if let Ok(Some(thumbnail)) = result {
                wrote_new = true;
                ready.push(thumbnail);
            }
        }
        if wrote_new {
            let cache_for_trim = cache.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || trim_cache(&cache_for_trim));
        }
    }

    Ok(ready
        .into_iter()
        .filter_map(|(path, file)| {
            register_local(&registry, file)
                .ok()
                .map(|media| ImageThumbnail {
                    path,
                    url: media.url,
                })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::convert::{encode::encode, formats::ImageFormat as Format};
    use image::{DynamicImage, Rgba, RgbaImage};

    fn avif_file(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, Rgba([20, 120, 220, 255])));
        let path = dir.join(name);
        fs::write(&path, encode(&image, Format::Avif, 80).unwrap()).unwrap();
        path
    }

    #[test]
    fn only_heic_heif_and_avif_need_decoding() {
        for name in ["a.heic", "A.HEIC", "b.heif", "c.avif", "/x/y/d.AVIF"] {
            assert!(needs_decode(Path::new(name)), "{name}");
        }
        for name in ["a.png", "b.jpg", "c.svg", "d", "e.heic.txt"] {
            assert!(!needs_decode(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn decoded_png_is_full_size_and_cached() {
        let dir = tempfile::tempdir().unwrap();
        let source = avif_file(dir.path(), "photo.avif", 300, 200);
        let cache = dir.path().join("cache");
        let first = decoded_png(&cache, &source).unwrap();
        assert_eq!(image::image_dimensions(&first).unwrap(), (300, 200));
        let stamp = fs::metadata(&first).unwrap().modified().unwrap();
        let second = decoded_png(&cache, &source).unwrap();
        assert_eq!(first, second);
        assert_eq!(fs::metadata(&second).unwrap().modified().unwrap(), stamp);
    }

    #[test]
    fn decoded_png_reports_unreadable_files() {
        let dir = tempfile::tempdir().unwrap();
        let broken = dir.path().join("broken.heic");
        fs::write(&broken, b"not a heic file").unwrap();
        assert!(decoded_png(&dir.path().join("cache"), &broken).is_err());
    }

    #[test]
    fn thumbnails_are_made_for_avif() {
        let dir = tempfile::tempdir().unwrap();
        let source = avif_file(dir.path(), "photo.avif", 600, 400);
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let (_, thumbnail) = make_thumbnail(&cache, source.to_str().unwrap(), None).unwrap();
        let (width, height) = image::image_dimensions(&thumbnail).unwrap();
        assert!(width <= EDGE && height <= EDGE && width.max(height) == EDGE, "{width}x{height}");
    }

    #[test]
    fn recognizes_video_extensions() {
        for name in ["clip.mp4", "CLIP.MOV", "movie.mkv", "a/b/c.webm", "x.m4v"] {
            assert!(is_video(Path::new(name)), "{name}");
        }
        for name in ["photo.png", "song.mp3", "doc.pdf", "noext"] {
            assert!(!is_video(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn thumbnails_are_made_for_video() {
        let tools = locate(None, &system_dirs());
        let Some(ref found) = tools else {
            eprintln!("SKIPPED: ffmpeg not installed to generate fixture video");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.mp4");
        let status = Command::new(&found.ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=duration=2:size=640x360:rate=25"])
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"])
            .arg(&source)
            .status()
            .unwrap();
        if !status.success() {
            return;
        }
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let (_, thumbnail) = make_thumbnail(&cache, source.to_str().unwrap(), tools.as_ref()).unwrap();
        let (width, height) = image::image_dimensions(&thumbnail).unwrap();
        assert!(width <= EDGE && height <= EDGE && width.max(height) == EDGE, "{width}x{height}");
    }

    /// Manual check: `LITE_EXPLORER_HEIC_SAMPLE=/path/photo.heic cargo test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_heic_preview_and_thumbnail() {
        let source = PathBuf::from(std::env::var("LITE_EXPLORER_HEIC_SAMPLE").expect("sample path"));
        let dir = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let png = decoded_png(dir.path(), &source).unwrap();
        println!(
            "preview: {:?}, {} bytes, {:?}",
            image::image_dimensions(&png).unwrap(),
            fs::metadata(&png).unwrap().len(),
            started.elapsed()
        );
        let cache = dir.path().join("thumbs");
        fs::create_dir_all(&cache).unwrap();
        let started = std::time::Instant::now();
        let (_, thumbnail) = make_thumbnail(&cache, source.to_str().unwrap(), None).unwrap();
        println!("thumbnail: {:?}, {:?}", image::image_dimensions(&thumbnail).unwrap(), started.elapsed());
    }

    #[test]
    fn cache_key_uses_revision_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let metadata = file.as_file().metadata().unwrap();
        assert_eq!(cache_key(file.path(), &metadata).len(), 64);
    }

    #[test]
    fn resolve_cached_or_pending_separates_hits_from_misses() {
        let dir = tempfile::tempdir().unwrap();
        let source = avif_file(dir.path(), "photo.avif", 120, 80);
        let cache = dir.path().join("cache");
        fs::create_dir_all(&cache).unwrap();
        let source_str = source.to_str().unwrap();

        // First check: cache miss returns Err with pending info
        assert!(resolve_cached_or_pending(&cache, source_str).is_err());

        // Generate thumbnail
        let (_, generated) = make_thumbnail(&cache, source_str, None).unwrap();

        // Second check: cache hit returns Ok(Some((source, target))) immediately
        let hit = resolve_cached_or_pending(&cache, source_str).unwrap().unwrap();
        assert_eq!(hit.0, source_str);
        assert_eq!(hit.1, generated);
    }
}
