//! Native file-type icons.
//!
//! Extracts the operating system's own icon for a file path and returns it as a
//! PNG data URL. macOS uses `NSWorkspace`, Windows uses the shell, and Linux
//! resolves the freedesktop MIME type through the icon theme.
//!
//! `file_icons` is batch-only: the frontend sends every path it needs in one
//! call and gets one entry per path back, so a single unreachable path never
//! fails the rest.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use base64::Engine;

type Icon = Option<String>;

static CACHE: OnceLock<Mutex<HashMap<String, Icon>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Icon>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// PNG data URL of the OS icon for `path`, cached per path.
pub(crate) fn icon_data_url(path: &str) -> Icon {
    if let Some(icon) = cache().lock().ok().and_then(|c| c.get(path).cloned()) {
        return icon;
    }
    let icon = platform_icon(path);
    if let Ok(mut c) = cache().lock() {
        c.insert(path.to_owned(), icon.clone());
    }
    icon
}

/// Whether an icon depends only on the extension (safe to share across paths).
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn extension_of(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

/// Extensions whose icon embeds per-file data (executables, shortcuts, …) and
/// must not be shared across paths.
#[cfg(target_os = "windows")]
const PER_FILE_EXTENSIONS: &[&str] = &[
    "exe",
    "lnk",
    "ico",
    "scr",
    "msi",
    "appref-ms",
    "url",
    "bat",
    "cmd",
    "com",
];

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn extension_cache() -> &'static Mutex<HashMap<String, Icon>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Icon>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn png_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(target_os = "macos")]
fn platform_icon(path: &str) -> Icon {
    use objc2::{
        class,
        encode::{Encode, Encoding, RefEncode},
        msg_send,
        rc::autoreleasepool,
        runtime::AnyObject,
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
    struct CGPoint {
        x: f64,
        y: f64,
    }
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }

    #[repr(C)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding =
            Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }
    unsafe impl RefEncode for CGRect {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Self::ENCODING);
    }

    #[repr(C)]
    struct CGImage {
        _private: [u8; 0],
    }
    unsafe impl RefEncode for CGImage {
        const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGImage", &[]));
    }

    let c_path = CString::new(path).ok()?;
    autoreleasepool(|_| unsafe {
        let string: *mut AnyObject =
            msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        let image: *mut AnyObject = msg_send![workspace, iconForFile: string];
        if image.is_null() {
            return None;
        }
        // 32pt at 2x keeps the 16px row icon sharp on Retina displays.
        let _: () = msg_send![image, setSize: CGSize { width: 32.0, height: 32.0 }];
        let null_object: *mut AnyObject = std::ptr::null_mut();
        let cg_image: *mut CGImage = msg_send![
            image,
            CGImageForProposedRect: std::ptr::null_mut::<CGRect>(),
            context: null_object,
            hints: null_object
        ];
        if cg_image.is_null() {
            return None;
        }
        let rep: *mut AnyObject = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut AnyObject = msg_send![rep, initWithCGImage: cg_image];
        if rep.is_null() {
            return None;
        }
        let properties: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        // NSBitmapImageFileTypePNG
        let data: *mut AnyObject =
            msg_send![rep, representationUsingType: 4usize, properties: properties];
        let encoded = if data.is_null() {
            None
        } else {
            let bytes: *const c_void = msg_send![data, bytes];
            let length: usize = msg_send![data, length];
            let slice = std::slice::from_raw_parts(bytes as *const u8, length);
            Some(png_data_url(slice))
        };
        let _: () = msg_send![rep, release];
        encoded
    })
}

#[cfg(target_os = "windows")]
fn platform_icon(path: &str) -> Icon {
    let extension = extension_of(path);
    let shareable = extension
        .as_deref()
        .map(|extension| !PER_FILE_EXTENSIONS.contains(&extension))
        .unwrap_or(false);
    if shareable {
        if let Some(icon) = extension
            .as_deref()
            .and_then(|extension| extension_cache().lock().ok()?.get(extension).cloned())
        {
            return icon;
        }
    }
    let icon = windows_icon(path);
    if shareable {
        if let (Some(extension), Ok(mut c)) = (extension, extension_cache().lock()) {
            c.insert(extension, icon.clone());
        }
    }
    icon
}

#[cfg(target_os = "windows")]
fn windows_icon(path: &str) -> Icon {
    use std::{ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::UI::Shell::{
        SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_USEFILEATTRIBUTES,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    let wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut info = SHFILEINFOW::default();
    let mut flags = SHGFI_ICON | SHGFI_LARGEICON;
    if !Path::new(path).exists() {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    let result = unsafe {
        SHGetFileInfoW(
            wide.as_ptr(),
            FILE_ATTRIBUTE_NORMAL,
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if result == 0 || info.hIcon.is_null() {
        return None;
    }

    let mut icon_info = ICONINFO::default();
    if unsafe { GetIconInfo(info.hIcon, &mut icon_info) } == 0 {
        unsafe { DestroyIcon(info.hIcon) };
        return None;
    }

    let mut bitmap = BITMAP::default();
    let got_bitmap = unsafe {
        GetObjectW(
            icon_info.hbmColor,
            size_of::<BITMAP>() as i32,
            &mut bitmap as *mut _ as *mut _,
        )
    };
    let (width, height) = if got_bitmap != 0 && bitmap.bmWidth > 0 && bitmap.bmHeight > 0 {
        (bitmap.bmWidth, bitmap.bmHeight)
    } else {
        (32, 32)
    };

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    bmi.bmiHeader.biWidth = width;
    // Negative height asks for a top-down bitmap, matching PNG row order.
    bmi.bmiHeader.biHeight = -height;
    bmi.bmiHeader.biPlanes = 1;
    bmi.bmiHeader.biBitCount = 32;
    bmi.bmiHeader.biCompression = BI_RGB;

    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
    let dc = unsafe { CreateCompatibleDC(ptr::null_mut()) };
    let previous = unsafe { SelectObject(dc, icon_info.hbmColor) };
    let lines = unsafe {
        GetDIBits(
            dc,
            icon_info.hbmColor,
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
        DeleteObject(icon_info.hbmColor);
        DeleteObject(icon_info.hbmMask);
        DestroyIcon(info.hIcon);
    }
    if lines == 0 {
        return None;
    }

    // BGRA without premultiplication -> straight RGBA.
    let opaque = buffer.chunks_exact(4).all(|pixel| pixel[3] == 0);
    let mut rgba = Vec::with_capacity(buffer.len());
    for pixel in buffer.chunks_exact(4) {
        rgba.push(pixel[2]);
        rgba.push(pixel[1]);
        rgba.push(pixel[0]);
        rgba.push(if opaque { 255 } else { pixel[3] });
    }

    encode_png(&rgba, width as u32, height as u32)
}

#[cfg(target_os = "windows")]
fn encode_png(rgba: &[u8], width: u32, height: u32) -> Icon {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(png_data_url(&bytes))
}

#[cfg(target_os = "linux")]
fn platform_icon(path: &str) -> Icon {
    let extension = extension_of(path);
    if let Some(icon) = extension
        .as_deref()
        .and_then(|extension| extension_cache().lock().ok()?.get(extension).cloned())
    {
        return icon;
    }
    let icon = linux_icon(path);
    if let (Some(extension), Ok(mut c)) = (extension, extension_cache().lock()) {
        c.insert(extension, icon.clone());
    }
    icon
}

#[cfg(target_os = "linux")]
fn linux_icon(path: &str) -> Icon {
    use gio::glib::Cast;

    // gdk-pixbuf is not thread-safe: serialize concurrent batches.
    let _guard = PIXBUF_LOCK.lock().ok();

    let (content_type, _) = gio::functions::content_type_guess(Some(path), &[]);
    let mut names = Vec::new();
    let icon = gio::functions::content_type_get_icon(content_type.as_str());
    if let Ok(themed) = icon.downcast::<gio::ThemedIcon>() {
        for name in themed.names() {
            names.push(name.to_string());
        }
    }
    if let Some(generic) = gio::functions::content_type_get_generic_icon_name(content_type.as_str())
    {
        names.push(generic.to_string());
    }

    let file = find_icon_file(&names)?;
    let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(&file, 48, 48, true).ok()?;
    let bytes = pixbuf.save_to_bufferv("png", &[]).ok()?;
    Some(png_data_url(&bytes))
}

/// PNG data URL for a GIO icon (an application's `Icon=` entry), cached by its
/// serialized form.
#[cfg(target_os = "linux")]
pub(crate) fn gicon_data_url(icon: &gio::Icon) -> Icon {
    use gio::glib::Cast;
    use gio::prelude::{FileExt, IconExt};

    let key = format!("gicon:{}", IconExt::to_string(icon)?);
    if let Some(cached) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return cached;
    }

    let file = if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>() {
        let names: Vec<String> = themed.names().iter().map(|name| name.to_string()).collect();
        find_icon_file(&names)
    } else if let Some(file_icon) = icon.downcast_ref::<gio::FileIcon>() {
        file_icon.file().path().filter(|path| path.is_file())
    } else {
        None
    };
    let encoded = file.and_then(|file| {
        // gdk-pixbuf is not thread-safe: serialize with file-type icon batches.
        let _guard = PIXBUF_LOCK.lock().ok();
        let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(&file, 32, 32, true).ok()?;
        let bytes = pixbuf.save_to_bufferv("png", &[]).ok()?;
        Some(png_data_url(&bytes))
    });

    if let Ok(mut c) = cache().lock() {
        c.insert(key, encoded.clone());
    }
    encoded
}

#[cfg(target_os = "linux")]
static PIXBUF_LOCK: Mutex<()> = Mutex::new(());

#[cfg(target_os = "linux")]
const ICON_SIZES: &[&str] = &[
    "scalable", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32", "24x24", "22x22", "16x16",
];

#[cfg(target_os = "linux")]
const ICON_THEMES: &[&str] = &["hicolor", "Adwaita", "gnome", "breeze", "Yaru"];
#[cfg(target_os = "linux")]
const ICON_CONTEXTS: &[&str] = &["mimetypes", "apps"];

#[cfg(target_os = "linux")]
fn icon_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/icons"));
        roots.push(home.join(".icons"));
    }
    match std::env::var_os("XDG_DATA_DIRS") {
        Some(dirs) => {
            for dir in std::env::split_paths(&dirs) {
                roots.push(dir.join("icons"));
            }
        }
        None => {
            roots.push(PathBuf::from("/usr/local/share/icons"));
            roots.push(PathBuf::from("/usr/share/icons"));
        }
    }
    roots.push(PathBuf::from("/usr/share/pixmaps"));
    roots
}

#[cfg(target_os = "linux")]
fn find_icon_file(names: &[String]) -> Option<PathBuf> {
    let roots = icon_roots();
    for name in names {
        for root in &roots {
            for extension in ["png", "svg"] {
                let flat = root.join(format!("{name}.{extension}"));
                if flat.is_file() {
                    return Some(flat);
                }
            }
            for theme in ICON_THEMES {
                for size in ICON_SIZES {
                    for context in ICON_CONTEXTS {
                        for extension in ["png", "svg"] {
                            let path = root
                                .join(theme)
                                .join(size)
                                .join(context)
                                .join(format!("{name}.{extension}"));
                            if path.is_file() {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn platform_icon(_path: &str) -> Icon {
    None
}

/// One entry per requested path, in the same order. `None` when an icon could
/// not be produced for that path.
#[tauri::command]
pub async fn file_icons(paths: Vec<String>) -> Vec<Icon> {
    let count = paths.len();
    tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|path| icon_data_url(path)).collect()
    })
    .await
    .unwrap_or_else(|_| vec![None; count])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_returns_a_png_data_url() {
        let icon = icon_data_url("/System/Applications/Calculator.app")
            .expect("app bundle should have an icon");
        assert!(icon.starts_with("data:image/png;base64,"));
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn extension_is_lowercased() {
        assert_eq!(extension_of("/tmp/REPORT.PDF").as_deref(), Some("pdf"));
        assert_eq!(extension_of("/tmp/README"), None);
    }
}
