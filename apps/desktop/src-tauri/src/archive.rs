//! Local archive creation and extraction: `.zip`, `.tar`, `.tar.gz` / `.tgz`.
//!
//! Both directions run on a blocking task. Extraction rejects paths that escape
//! the destination (zip `enclosed_name`, tar's built-in sanitizer).

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Zip,
    Tar,
    TarGz,
}

fn format_for(path: &Path) -> Option<Format> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".zip") {
        Some(Format::Zip)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(Format::TarGz)
    } else if name.ends_with(".tar") {
        Some(Format::Tar)
    } else {
        None
    }
}

pub fn create(paths: &[PathBuf], destination: &Path) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing to compress".into());
    }
    for path in paths {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
    }
    match format_for(destination).ok_or("Unsupported format. Use .zip, .tar, .tar.gz or .tgz")? {
        Format::Zip => create_zip(paths, destination),
        Format::Tar => create_tar(paths, destination, false),
        Format::TarGz => create_tar(paths, destination, true),
    }
}

fn create_zip(paths: &[PathBuf], destination: &Path) -> Result<(), String> {
    let file = File::create(destination).map_err(|error| error.to_string())?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for path in paths {
        let root = path
            .file_name()
            .ok_or("Invalid path to compress")?
            .to_string_lossy()
            .into_owned();
        add_zip_entry(&mut zip, path, Path::new(&root), &options)?;
    }
    zip.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn add_zip_entry(
    zip: &mut ZipWriter<File>,
    source: &Path,
    rel: &Path,
    options: &SimpleFileOptions,
) -> Result<(), String> {
    if source.is_dir() {
        let dir = format!("{}/", rel.to_string_lossy().replace('\\', "/"));
        zip.add_directory(dir, *options)
            .map_err(|error| error.to_string())?;
        for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let child = rel.join(entry.file_name());
            add_zip_entry(zip, &entry.path(), &child, options)?;
        }
    } else {
        let name = rel.to_string_lossy().replace('\\', "/");
        zip.start_file(name, *options)
            .map_err(|error| error.to_string())?;
        let mut file = File::open(source).map_err(|error| error.to_string())?;
        io::copy(&mut file, zip).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn create_tar(paths: &[PathBuf], destination: &Path, gzip: bool) -> Result<(), String> {
    let file = File::create(destination).map_err(|error| error.to_string())?;
    let writer: Box<dyn Write> = if gzip {
        Box::new(GzEncoder::new(file, Compression::default()))
    } else {
        Box::new(file)
    };
    let mut builder = tar::Builder::new(writer);
    for path in paths {
        let root = path
            .file_name()
            .ok_or("Invalid path to compress")?
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            builder
                .append_dir_all(&root, path)
                .map_err(|error| error.to_string())?;
        } else {
            builder
                .append_path_with_name(path, &root)
                .map_err(|error| error.to_string())?;
        }
    }
    builder.finish().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn extract(archive: &Path, destination: &Path) -> Result<(), String> {
    if !archive.exists() {
        return Err(format!("{} does not exist", archive.display()));
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    match format_for(archive).ok_or("Unsupported format. Use .zip, .tar, .tar.gz or .tgz")? {
        Format::Zip => extract_zip(archive, destination),
        Format::Tar => extract_tar(archive, destination, false),
        Format::TarGz => extract_tar(archive, destination, true),
    }
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let mut zip = ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
        let enclosed = entry
            .enclosed_name()
            .ok_or("Archive contains an unsafe path")?
            .to_path_buf();
        let out = destination.join(enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = File::create(&out).map_err(|error| error.to_string())?;
            io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn extract_tar(archive: &Path, destination: &Path, gzip: bool) -> Result<(), String> {
    let file = File::open(archive).map_err(|error| error.to_string())?;
    let reader: Box<dyn Read> = if gzip {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut tar = tar::Archive::new(reader);
    tar.set_overwrite(true);
    tar.unpack(destination).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn create_archive(paths: Vec<String>, destination: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        create(&paths, &PathBuf::from(destination))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn extract_archive(path: String, destination: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        extract(&PathBuf::from(path), &PathBuf::from(destination))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lite-explorer-archive-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_formats() {
        assert_eq!(format_for(Path::new("a.zip")), Some(Format::Zip));
        assert_eq!(format_for(Path::new("a.TAR.GZ")), Some(Format::TarGz));
        assert_eq!(format_for(Path::new("a.tgz")), Some(Format::TarGz));
        assert_eq!(format_for(Path::new("a.tar")), Some(Format::Tar));
        assert_eq!(format_for(Path::new("a.txt")), None);
    }

    #[test]
    fn zip_round_trip() {
        let dir = temp_dir("zip");
        let source = dir.join("hello.txt");
        fs::write(&source, b"hi there").unwrap();
        let archive = dir.join("out.zip");
        create(&[source.clone()], &archive).unwrap();
        let out = dir.join("out");
        extract(&archive, &out).unwrap();
        assert_eq!(fs::read(out.join("hello.txt")).unwrap(), b"hi there");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tar_gz_round_trip_preserves_folders() {
        let dir = temp_dir("targz");
        let folder = dir.join("docs");
        fs::create_dir_all(&folder).unwrap();
        let mut file = File::create(folder.join("note.md")).unwrap();
        file.write_all(b"# note").unwrap();
        drop(file);
        let archive = dir.join("out.tar.gz");
        create(&[folder], &archive).unwrap();
        let out = dir.join("out");
        extract(&archive, &out).unwrap();
        assert_eq!(fs::read(out.join("docs/note.md")).unwrap(), b"# note");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_unknown_format() {
        let dir = temp_dir("bad");
        let source = dir.join("x.txt");
        fs::write(&source, b"x").unwrap();
        assert!(create(&[source], &dir.join("out.rar")).is_err());
        fs::remove_dir_all(&dir).ok();
    }
}
