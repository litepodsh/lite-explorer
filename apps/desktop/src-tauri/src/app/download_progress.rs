//! Read-only snapshots of download metadata. Never infer a percentage from file size.
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    path: String,
    fraction: Option<f64>,
    bytes_done: Option<u64>,
    missing: bool,
}

#[cfg(any(target_os = "macos", test))]
fn parse_fraction(bytes: &[u8]) -> Option<f64> {
    let value: f64 = std::str::from_utf8(bytes).ok()?.trim().parse().ok()?;
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value)
}

#[cfg(target_os = "macos")]
fn fraction(path: &Path) -> Option<f64> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut buffer = [0u8; 64];
    // Safari writes an ASCII fraction on its .download bundle, also used by Finder.
    // ponytail: this metadata is browser-owned; use NSProgress subscriptions if a
    // browser publishes live progress without this attribute.
    let count = unsafe {
        libc::getxattr(
            path.as_ptr(),
            c"com.apple.progress.fractionCompleted".as_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };
    if count <= 0 {
        return None;
    }
    parse_fraction(&buffer[..count as usize])
}

#[cfg(not(target_os = "macos"))]
fn fraction(_path: &Path) -> Option<f64> {
    None
}

fn snapshot(path: String) -> DownloadProgress {
    let file = Path::new(&path);
    let metadata = file.symlink_metadata();
    let missing = metadata
        .as_ref()
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
    let fraction = fraction(file);
    let bytes_done = metadata.ok().and_then(|meta| {
        if meta.is_file() {
            return Some(meta.len());
        }
        // Safari's bundle contains the payload with the .download suffix removed.
        // Read that one file; don't recursively walk a download directory.
        if meta.is_dir() && file.extension().is_some_and(|ext| ext == "download") {
            let payload = file.join(file.file_stem()?);
            return payload
                .symlink_metadata()
                .ok()
                .filter(|meta| meta.is_file())
                .map(|meta| meta.len());
        }
        None
    });
    DownloadProgress {
        path,
        fraction,
        bytes_done,
        missing,
    }
}

#[tauri::command]
pub async fn read_download_progress(paths: Vec<String>) -> Result<Vec<DownloadProgress>, String> {
    if paths.len() > 512
        || paths
            .iter()
            .any(|path| !Path::new(path).is_absolute() || path.contains('\0'))
    {
        return Err("Expected at most 512 absolute local paths".into());
    }
    tauri::async_runtime::spawn_blocking(move || paths.into_iter().map(snapshot).collect())
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_reject_invalid_and_unknown_values() {
        assert_eq!(parse_fraction(b"0.251"), Some(0.251));
        assert_eq!(parse_fraction(b" 1\n"), Some(1.0));
        assert_eq!(parse_fraction(b"0"), Some(0.0));
        for value in ["", "NaN", "inf", "-1", "1.01", "25%"] {
            assert_eq!(parse_fraction(value.as_bytes()), None);
        }
    }

    /// Read-only manual check: DOWNLOAD_PROGRESS_PATH=/absolute/file cargo test
    /// download_progress::tests::inspect_download -- --ignored --nocapture
    #[test]
    #[ignore]
    fn inspect_download() {
        let result =
            snapshot(std::env::var("DOWNLOAD_PROGRESS_PATH").expect("Set DOWNLOAD_PROGRESS_PATH"));
        println!("{}", serde_json::to_string(&result).unwrap());
        assert!(!result.missing);
        assert!(result.fraction.is_some());
    }
}
