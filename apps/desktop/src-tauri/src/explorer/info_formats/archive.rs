//! Entry counts and sizes for zip and tar archives, read from the zip directory and tar headers.
use super::{clip, format_size, group, InfoSection, Zip};
use crate::explorer::archive::{decoder, Codec};
use std::{
    fs::File,
    io::{self, BufReader, Read},
    time::{Duration, Instant},
};

pub(crate) fn zip_section(zip: &mut Zip) -> Option<InfoSection> {
    let (mut files, mut folders, mut size, mut compressed, mut encrypted) =
        (0u64, 0u64, 0u64, 0u64, false);
    for index in 0..zip.len() {
        let Ok(entry) = zip.by_index_raw(index) else {
            continue;
        };
        if entry.is_dir() {
            folders += 1;
        } else {
            files += 1;
        }
        size = size.saturating_add(entry.size());
        compressed = compressed.saturating_add(entry.compressed_size());
        encrypted |= entry.encrypted();
    }
    let mut section = InfoSection::new("Archive");
    section.push("Format", "ZIP");
    section.push("Contents", contents(files, folders));
    section.push("Uncompressed size", format_size(size));
    if size > 0 {
        section.push("Compression", ratio(compressed, size));
    }
    if encrypted {
        section.push("Encrypted", "Yes");
    }
    section.push("Comment", clip(&String::from_utf8_lossy(zip.comment())));
    Some(section)
}

fn contents(files: u64, folders: u64) -> String {
    let count =
        |n: u64, one: &str, many: &str| format!("{} {}", group(n), if n == 1 { one } else { many });
    format!(
        "{}, {}",
        count(files, "file", "files"),
        count(folders, "folder", "folders")
    )
}

fn ratio(compressed: u64, size: u64) -> String {
    format!(
        "{}% of original",
        (compressed as f64 * 100.0 / size as f64).round()
    )
}

struct TarSummary {
    files: u64,
    folders: u64,
    size: u64,
    complete: bool,
}

/// Counts entries until the end, an error, `limit` entries or `deadline`; only the first sets `complete`.
fn summarize<'a, R: Read + 'a>(
    entries: io::Result<tar::Entries<'a, R>>,
    limit: usize,
    deadline: Instant,
) -> TarSummary {
    let mut summary = TarSummary {
        files: 0,
        folders: 0,
        size: 0,
        complete: false,
    };
    let Ok(entries) = entries else { return summary };
    for (seen, entry) in entries.enumerate() {
        if seen >= limit || Instant::now() >= deadline {
            return summary;
        }
        let Ok(entry) = entry else { return summary };
        let header = entry.header();
        if header.entry_type().is_dir() {
            summary.folders += 1;
        } else {
            summary.files += 1;
            summary.size = summary.size.saturating_add(header.size().unwrap_or(0));
        }
    }
    summary.complete = true;
    summary
}

pub(crate) fn tar_section(file: File, limit: usize, time: Duration) -> Option<InfoSection> {
    let mut archive = tar::Archive::new(BufReader::new(file));
    let summary = summarize(archive.entries_with_seek(), limit, Instant::now() + time);
    render("TAR", &summary, None)
}

pub(crate) fn compressed_tar_section(
    file: File,
    codec: Codec,
    length: u64,
    limit: usize,
    time: Duration,
) -> Option<InfoSection> {
    let mut archive = tar::Archive::new(decoder(codec, BufReader::new(file)).ok()?);
    let summary = summarize(archive.entries(), limit, Instant::now() + time);
    let name = match codec {
        Codec::Gzip => "gzip",
        Codec::Bzip2 => "bzip2",
        Codec::Xz => "xz",
        Codec::Zstd => "zstd",
    };
    render(&format!("TAR · {name}"), &summary, Some(length))
}

fn render(format: &str, summary: &TarSummary, compressed: Option<u64>) -> Option<InfoSection> {
    if !summary.complete && summary.files + summary.folders == 0 {
        return None;
    }
    let mut section = InfoSection::new("Archive");
    section.push("Format", format);
    if summary.complete {
        section.push("Contents", contents(summary.files, summary.folders));
        section.push("Uncompressed size", format_size(summary.size));
        if let Some(compressed) = compressed.filter(|_| summary.size > 0) {
            section.push("Compression", ratio(compressed, summary.size));
        }
    } else {
        section.push(
            "Contents",
            format!("≥ {} items", group(summary.files + summary.folders)),
        );
        section.push(
            "Uncompressed size",
            format!("≥ {}", format_size(summary.size)),
        );
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{rows, rows_for, zip_bytes};
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::io::{Cursor, Write};

    fn tar_bytes(files: usize, name_length: usize) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        let mut folder = tar::Header::new_gnu();
        folder.set_entry_type(tar::EntryType::Directory);
        folder.set_size(0);
        folder.set_mode(0o755);
        builder
            .append_data(&mut folder, "docs/", io::empty())
            .unwrap();
        for index in 0..files {
            let mut header = tar::Header::new_gnu();
            header.set_size(1500);
            header.set_mode(0o644);
            let name = format!("docs/{index}-{}.txt", "n".repeat(name_length));
            builder
                .append_data(&mut header, name, &[b'x'; 1500][..])
                .unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    /// Sets the "encrypted" flag bit on every local and central zip header.
    fn mark_encrypted(bytes: &mut [u8]) {
        for index in 0..bytes.len().saturating_sub(10) {
            match &bytes[index..index + 4] {
                b"PK\x03\x04" => bytes[index + 6] |= 1,
                b"PK\x01\x02" => bytes[index + 8] |= 1,
                _ => {}
            }
        }
    }

    #[test]
    fn zip_counts_sizes_flags_and_comment() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        zip.add_directory("docs/", options).unwrap();
        zip.start_file("docs/a.txt", options).unwrap();
        zip.write_all(&[b'a'; 1000]).unwrap();
        zip.set_comment("Backup de marzo");
        let mut bytes = zip.finish().unwrap().into_inner();
        mark_encrypted(&mut bytes);
        let found = rows_for(&bytes, "backup.zip", "Archive");
        assert_eq!(
            found[..3],
            rows(&[
                ("Format", "ZIP"),
                ("Contents", "1 file, 1 folder"),
                ("Uncompressed size", "1 KB")
            ])[..]
        );
        let compression = &found[3];
        assert_eq!(compression.0, "Compression");
        assert!(compression.1.ends_with("% of original"));
        assert_eq!(
            found[4..],
            rows(&[("Encrypted", "Yes"), ("Comment", "Backup de marzo")])[..]
        );
    }

    #[test]
    fn zip_named_docx_without_office_parts_is_an_archive() {
        let bytes = zip_bytes(&[("notes.txt", "hola")]);
        let found = rows_for(&bytes, "notes.docx", "Archive");
        assert_eq!(found[0], ("Format".into(), "ZIP".into()));
        assert!(rows_for(&bytes, "notes.docx", "Document").is_empty());
    }

    #[test]
    fn plain_and_gzip_tar() {
        let tar = tar_bytes(2, 0);
        assert_eq!(
            rows_for(&tar, "backup.tar", "Archive"),
            rows(&[
                ("Format", "TAR"),
                ("Contents", "2 files, 1 folder"),
                ("Uncompressed size", "3 KB"),
            ])
        );
        let found = rows_for(&gzip(&tar), "backup.tar.gz", "Archive");
        assert_eq!(
            found[..3],
            rows(&[
                ("Format", "TAR · gzip"),
                ("Contents", "2 files, 1 folder"),
                ("Uncompressed size", "3 KB")
            ])[..]
        );
        assert!(found[3].1.ends_with("% of original"));
        assert!(rows_for(&gzip(b"just a log line"), "app.log.gz", "Archive").is_empty());
    }

    #[test]
    fn tar_limit_shows_lower_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.tar.gz");
        let bytes = gzip(&tar_bytes(5, 0));
        std::fs::write(&path, &bytes).unwrap();
        let section = compressed_tar_section(
            File::open(&path).unwrap(),
            Codec::Gzip,
            bytes.len() as u64,
            2,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            section.rows,
            rows(&[
                ("Format", "TAR · gzip"),
                ("Contents", "≥ 2 items"),
                ("Uncompressed size", "≥ 1.5 KB")
            ])
        );
    }

    #[test]
    fn tar_long_names_count_once() {
        let found = rows_for(&tar_bytes(1, 150), "long.tar", "Archive");
        assert_eq!(found[1], ("Contents".into(), "1 file, 1 folder".into()));
    }
}
