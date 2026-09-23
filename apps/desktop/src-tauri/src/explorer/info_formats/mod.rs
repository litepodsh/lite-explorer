//! Format sections for Get Info: detection by signature, then one bounded reader per format.
//! A reader that fails leaves its section out; it never fails the whole inspection.
use super::archive::Codec;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone};
use serde::Serialize;
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};
use zip::ZipArchive;

mod archive;
mod audio;
mod epub;
mod image;
mod office;
mod photo;

const HEAD_BYTES: u64 = 512;
const TEXT_MAX_CHARS: usize = 500;
const RAW_EXTENSIONS: [&str; 4] = [".dng", ".cr2", ".nef", ".arw"];
const HEIF_BRANDS: [&[u8]; 6] = [b"heic", b"heix", b"hevc", b"heim", b"mif1", b"avif"];
const BMP_HEADER_SIZES: [u32; 6] = [12, 40, 52, 56, 108, 124];
const ZIP_PART_MAX: u64 = 2 * 1024 * 1024;
pub(crate) const TAR_ENTRY_LIMIT: usize = 10_000;
pub(crate) const TAR_TIME_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

pub(crate) type Zip = ZipArchive<BufReader<File>>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DocKind {
    Text,
    Spreadsheet,
    Presentation,
    Other,
}

enum ZipKind {
    Epub,
    Odf(DocKind),
    Ooxml(DocKind),
    Plain,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct InfoSection {
    pub title: String,
    pub rows: Vec<(String, String)>,
}

impl InfoSection {
    pub(crate) fn new(title: &str) -> Self {
        Self {
            title: title.into(),
            rows: Vec::new(),
        }
    }

    /// Adds a row unless the value is blank.
    pub(crate) fn push(&mut self, label: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.trim().is_empty() {
            self.rows.push((label.into(), value));
        }
    }

    pub(crate) fn push_opt(&mut self, label: &str, value: Option<impl Into<String>>) {
        if let Some(value) = value {
            self.push(label, value);
        }
    }

    pub(crate) fn non_empty(self) -> Option<Self> {
        (!self.rows.is_empty()).then_some(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    Png,
    Gif,
    Jpeg,
    Webp,
    Bmp,
    Tiff,
    Psd,
    Svg,
    Heif,
    Wav,
    Zip,
    Cfb,
    Tar,
    Compressed(Codec),
}

/// Kind of file from its first bytes; the name only decides SVG.
pub(crate) fn detect(head: &[u8], lower_name: &str) -> Option<Kind> {
    let starts = |magic: &[u8]| head.starts_with(magic);
    let riff = |form: &[u8]| starts(b"RIFF") && head.get(8..12) == Some(form);
    Some(if starts(b"\x89PNG\r\n\x1a\n") {
        Kind::Png
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Kind::Gif
    } else if starts(&[0xff, 0xd8, 0xff]) {
        Kind::Jpeg
    } else if riff(b"WEBP") {
        Kind::Webp
    } else if riff(b"WAVE") {
        Kind::Wav
    } else if starts(b"BM") && le32(head, 14).is_some_and(|size| BMP_HEADER_SIZES.contains(&size)) {
        Kind::Bmp
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Kind::Tiff
    } else if starts(b"8BPS") {
        Kind::Psd
    } else if head.get(4..8) == Some(&b"ftyp"[..])
        && head
            .get(8..12)
            .is_some_and(|brand| HEIF_BRANDS.contains(&brand))
    {
        Kind::Heif
    } else if starts(b"PK\x03\x04") || starts(b"PK\x05\x06") {
        Kind::Zip
    } else if starts(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        Kind::Cfb
    } else if starts(&[0x1f, 0x8b]) {
        Kind::Compressed(Codec::Gzip)
    } else if starts(b"BZh") {
        Kind::Compressed(Codec::Bzip2)
    } else if starts(&[0xfd, b'7', b'z', b'X', b'Z', 0]) {
        Kind::Compressed(Codec::Xz)
    } else if starts(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Kind::Compressed(Codec::Zstd)
    } else if head.get(257..262) == Some(&b"ustar"[..]) {
        Kind::Tar
    } else if lower_name.ends_with(".svg") {
        Kind::Svg
    } else {
        return None;
    })
}

fn is_raw(lower_name: &str) -> bool {
    RAW_EXTENSIONS
        .iter()
        .any(|extension| lower_name.ends_with(extension))
}

fn is_compressed_tar(lower_name: &str) -> bool {
    matches!(
        super::archive::archive_kind(lower_name),
        Some(super::archive::ArchiveKind::Tar(Some(_)))
    )
}

/// A zip part as text, or `None` when missing or larger than 2 MiB.
pub(crate) fn read_zip_text(zip: &mut Zip, name: &str) -> Option<String> {
    let entry = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.take(ZIP_PART_MAX + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= ZIP_PART_MAX).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn classify_zip(zip: &mut Zip) -> ZipKind {
    if let Some(mime) = read_zip_text(zip, "mimetype") {
        let mime = mime.trim();
        if mime == "application/epub+zip" {
            return ZipKind::Epub;
        }
        if let Some(kind) = mime.strip_prefix("application/vnd.oasis.opendocument.") {
            return ZipKind::Odf(match kind {
                "text" => DocKind::Text,
                "spreadsheet" => DocKind::Spreadsheet,
                "presentation" => DocKind::Presentation,
                _ => DocKind::Other,
            });
        }
    }
    if zip.index_for_name("[Content_Types].xml").is_some() {
        for (part, kind) in [
            ("word/document.xml", DocKind::Text),
            ("xl/workbook.xml", DocKind::Spreadsheet),
            ("ppt/presentation.xml", DocKind::Presentation),
        ] {
            if zip.index_for_name(part).is_some() {
                return ZipKind::Ooxml(kind);
            }
        }
    }
    ZipKind::Plain
}

fn zip_section(file: File) -> Option<InfoSection> {
    let mut zip = ZipArchive::new(BufReader::new(file)).ok()?;
    match classify_zip(&mut zip) {
        ZipKind::Odf(kind) => office::odf_section(&mut zip, kind),
        ZipKind::Ooxml(kind) => office::ooxml_section(&mut zip, kind),
        ZipKind::Plain => archive::zip_section(&mut zip),
        ZipKind::Epub => epub::section(&mut zip),
    }
}

/// Format sections of a regular file, in display order.
pub(crate) fn sections(path: &Path) -> Vec<InfoSection> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut head = Vec::new();
    // Readers that stream from the current position (tar, compressed tar) need the file rewound.
    if (&mut file).take(HEAD_BYTES).read_to_end(&mut head).is_err()
        || file.seek(SeekFrom::Start(0)).is_err()
    {
        return Vec::new();
    }
    let lower_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(kind) = detect(&head, &lower_name) else {
        return Vec::new();
    };
    let mut sections = Vec::new();
    match kind {
        Kind::Wav => sections.extend(audio::section(&mut file)),
        Kind::Zip => sections.extend(zip_section(file)),
        Kind::Cfb => sections.extend(office::encrypted_section(file)),
        Kind::Tar => sections.extend(archive::tar_section(file, TAR_ENTRY_LIMIT, TAR_TIME_LIMIT)),
        Kind::Compressed(codec) if is_compressed_tar(&lower_name) => sections.extend(
            archive::compressed_tar_section(file, codec, length, TAR_ENTRY_LIMIT, TAR_TIME_LIMIT),
        ),
        Kind::Compressed(_) | Kind::Heif => {}
        Kind::Tiff if is_raw(&lower_name) => {}
        image_kind => sections.extend(image::section(image_kind, &head, &mut file)),
    }
    if matches!(
        kind,
        Kind::Jpeg | Kind::Tiff | Kind::Png | Kind::Webp | Kind::Heif
    ) {
        sections.extend(photo::section(path, length));
    }
    sections
}

pub(crate) fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}
pub(crate) fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}
pub(crate) fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}
pub(crate) fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let part = bytes.get(at..at + 3)?;
    Some(u32::from_le_bytes([part[0], part[1], part[2], 0]))
}
pub(crate) fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// `1234567` → `1,234,567`.
pub(crate) fn group(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// One decimal, dropped when zero: `1.8`, `8`.
pub(crate) fn one_decimal(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// Mirrors `formatSize` in `src/lib/components/custom/preview/format.ts`.
pub(crate) fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1000 {
        return if bytes == 1 {
            "1 byte".into()
        } else {
            format!("{bytes} bytes")
        };
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    format!("{} {}", one_decimal(value), UNITS[unit])
}

/// Trims and cuts document text to 500 characters.
pub(crate) fn clip(value: &str) -> String {
    let value = value.trim();
    match value.char_indices().nth(TEXT_MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &value[..cut]),
        None => value.to_string(),
    }
}

/// ISO 8601 date or date-time as local `YYYY-MM-DD HH:MM`; unknown shapes are shown as given.
pub(crate) fn format_date(value: &str) -> String {
    format_date_in(value, &chrono::Local)
}

fn format_date_in<Tz: TimeZone>(value: &str, zone: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let value = value.trim();
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return date
            .with_timezone(zone)
            .format("%Y-%m-%d %H:%M")
            .to_string();
    }
    if let Ok(date) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return date.format("%Y-%m-%d %H:%M").to_string();
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date.format("%Y-%m-%d").to_string();
    }
    clip(value)
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{Cursor, Write};

    /// Writes `bytes` as `name` in a temp folder and returns the rows of the section titled `title`.
    pub(crate) fn rows_for(bytes: &[u8], name: &str, title: &str) -> Vec<(String, String)> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        super::sections(&path)
            .into_iter()
            .find(|section| section.title == title)
            .map(|section| section.rows)
            .unwrap_or_default()
    }

    pub(crate) fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(label, value)| (label.to_string(), value.to_string()))
            .collect()
    }

    pub(crate) fn zip_bytes(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, body) in parts {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn detects_by_signature_before_extension() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(64, 0);
        assert_eq!(detect(&png, "notes.txt"), Some(Kind::Png));
        assert_eq!(detect(b"<svg/>", "icon.svg"), Some(Kind::Svg));
        assert_eq!(detect(b"<svg/>", "icon.txt"), None);
        let mut tar = vec![0u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert_eq!(detect(&tar, "backup"), Some(Kind::Tar));
        assert_eq!(
            detect(&[0x1f, 0x8b, 8, 0], "log.gz"),
            Some(Kind::Compressed(Codec::Gzip))
        );
        assert_eq!(
            detect(b"BM is not a bitmap when the header size is odd", "a.bmp"),
            None
        );
        let mut heic = vec![0, 0, 0, 24];
        heic.extend(b"ftypheic");
        assert_eq!(detect(&heic, "img.heic"), Some(Kind::Heif));
        assert_eq!(detect(b"", "empty"), None);
    }

    #[test]
    fn formats_counts_sizes_and_decimals() {
        assert_eq!(group(0), "0");
        assert_eq!(group(999), "999");
        assert_eq!(group(1_234_567), "1,234,567");
        assert_eq!(format_size(1), "1 byte");
        assert_eq!(format_size(999), "999 bytes");
        assert_eq!(format_size(1000), "1 KB");
        assert_eq!(format_size(1500), "1.5 KB");
        assert_eq!(format_size(1_234_567), "1.2 MB");
        assert_eq!(one_decimal(1.8), "1.8");
        assert_eq!(one_decimal(8.0), "8");
    }

    #[test]
    fn clips_long_text() {
        let long = "a".repeat(600);
        let clipped = clip(&long);
        assert_eq!(clipped.chars().count(), 501);
        assert!(clipped.ends_with('…'));
        assert_eq!(clip("  short  "), "short");
    }

    #[test]
    fn formats_dates() {
        assert_eq!(
            format_date_in("2024-03-01T14:22:00Z", &Utc),
            "2024-03-01 14:22"
        );
        assert_eq!(
            format_date_in("2024-03-01T14:22:00.5", &Utc),
            "2024-03-01 14:22"
        );
        assert_eq!(format_date_in("1967-05-30", &Utc), "1967-05-30");
        assert_eq!(format_date_in("sometime", &Utc), "sometime");
    }
}
