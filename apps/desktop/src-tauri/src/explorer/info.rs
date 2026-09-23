//! Lightweight file information: never loads a preview or recursively walks a folder.
use super::info_formats::{self, InfoSection};
use super::local_path::{validate_existing, ExpectedKind};
use crate::epoch_millis;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Serialize)]
pub struct FileInfo {
    size: Option<u64>,
    created: Option<u64>,
    modified: Option<u64>,
    accessed: Option<u64>,
    readonly: bool,
    details: Vec<(String, String)>,
    permissions: Vec<(String, String)>,
    sections: Vec<InfoSection>,
}

fn inspect(path: &Path) -> Result<FileInfo, String> {
    let path = validate_existing(path, ExpectedKind::Any)?;
    let meta = fs::metadata(&path).map_err(|error| error.to_string())?;
    let mut info = FileInfo {
        size: (!meta.is_dir()).then_some(meta.len()),
        created: epoch_millis(meta.created()),
        modified: epoch_millis(meta.modified()),
        accessed: epoch_millis(meta.accessed()),
        readonly: meta.permissions().readonly(),
        details: Vec::new(),
        permissions: Vec::new(),
        sections: Vec::new(),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        info.details.extend([
            ("Allocated bytes".into(), (meta.blocks() * 512).to_string()),
            ("File ID (inode)".into(), meta.ino().to_string()),
            ("Device ID".into(), meta.dev().to_string()),
            ("Hard links".into(), meta.nlink().to_string()),
        ]);
        info.permissions.extend([
            ("Owner ID".into(), meta.uid().to_string()),
            ("Group ID".into(), meta.gid().to_string()),
            ("Mode".into(), format!("{:04o}", meta.mode() & 0o7777)),
        ]);
        for (label, shift) in [("Owner", 6), ("Group", 3), ("Everyone", 0)] {
            let mode = (meta.mode() >> shift) & 7;
            let rights: Vec<&str> = [(4, "Read"), (2, "Write"), (1, "Execute")]
                .into_iter()
                .filter_map(|(bit, name)| (mode & bit != 0).then_some(name))
                .collect();
            info.permissions.push((
                label.into(),
                if rights.is_empty() {
                    "No access".into()
                } else {
                    rights.join(", ")
                },
            ));
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        info.details.push((
            "Attributes".into(),
            format!("0x{:08X}", meta.file_attributes()),
        ));
    }
    if meta.is_file() {
        info.sections = info_formats::sections(&path);
    }
    Ok(info)
}

#[tauri::command]
pub async fn read_file_info(path: String) -> Result<FileInfo, String> {
    tauri::async_runtime::spawn_blocking(move || inspect(Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinguishes_file_size_from_directory_metadata() {
        let dir = std::env::temp_dir().join(format!("lite-info-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&dir).unwrap();
        let file = dir.join("example.txt");
        fs::write(&file, b"hello").unwrap();
        let info = inspect(&file.canonicalize().unwrap()).unwrap();
        assert_eq!(info.size, Some(5));
        assert!(info.modified.is_some());
        assert_eq!(inspect(&dir.canonicalize().unwrap()).unwrap().size, None);
        assert!(inspect(Path::new("relative.txt")).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn format_sections_only_for_files() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("pixel.gif");
        fs::write(&image, b"GIF89a\x80\x02\xe0\x01").unwrap();
        let info = inspect(&image.canonicalize().unwrap()).unwrap();
        assert_eq!(info.sections[0].title, "Image");
        assert!(!info.details.iter().any(|(label, _)| label == "Dimensions"));
        assert!(inspect(&dir.path().canonicalize().unwrap())
            .unwrap()
            .sections
            .is_empty());
    }
}
