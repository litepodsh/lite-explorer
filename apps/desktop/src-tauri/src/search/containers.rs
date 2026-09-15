//! Containers content search opens: zip-based packages, tar archives under any common
//! compression, and single compressed files such as `app.log.gz`.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use bzip2::read::MultiBzDecoder;
use flate2::read::MultiGzDecoder;
use liblzma::read::XzDecoder;
use ruzstd::decoding::StreamingDecoder;

use crate::archive::{self, EntryInfo, Flow};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Codec {
    Gzip,
    Bzip2,
    Xz,
    Zstd,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ArchiveKind {
    Zip,
    Tar(Option<Codec>),
}

/// Packages that are plain zip files under another extension.
const ZIP_EXTENSIONS: &[&str] = &[
    "zip", "jar", "war", "ear", "aar", "apk", "aab", "ipa", "xpi", "vsix", "nupkg", "whl", "egg",
];

/// Archive kind by lowercased file name.
pub(super) fn archive_kind(lower_name: &str) -> Option<ArchiveKind> {
    const TARS: &[(&[&str], Option<Codec>)] = &[
        (&[".tar"], None),
        (&[".tar.gz", ".tgz"], Some(Codec::Gzip)),
        (&[".tar.bz2", ".tbz2", ".tbz"], Some(Codec::Bzip2)),
        (&[".tar.xz", ".txz"], Some(Codec::Xz)),
        (&[".tar.zst", ".tar.zstd", ".tzst"], Some(Codec::Zstd)),
    ];
    if let Some((_, codec)) = TARS.iter().find(|(suffixes, _)| suffixes.iter().any(|suffix| lower_name.ends_with(suffix))) {
        return Some(ArchiveKind::Tar(*codec));
    }
    let (_, extension) = lower_name.rsplit_once('.')?;
    ZIP_EXTENSIONS.contains(&extension).then_some(ArchiveKind::Zip)
}

/// Codec and inner file name of a single compressed file, e.g. `app.log.gz` -> (Gzip, `app.log`).
/// Call after [`archive_kind`], so compressed tars are already handled.
pub(super) fn compressed(name: &str) -> Option<(Codec, &str)> {
    let (stem, extension) = name.rsplit_once('.')?;
    let codec = match extension.to_ascii_lowercase().as_str() {
        "gz" => Codec::Gzip,
        "bz2" => Codec::Bzip2,
        "xz" => Codec::Xz,
        "zst" | "zstd" => Codec::Zstd,
        _ => return None,
    };
    (!stem.is_empty()).then_some((codec, stem))
}

pub(super) fn decoder<'a>(codec: Codec, reader: impl Read + 'a) -> Result<Box<dyn Read + 'a>, ()> {
    Ok(match codec {
        Codec::Gzip => Box::new(MultiGzDecoder::new(reader)),
        Codec::Bzip2 => Box::new(MultiBzDecoder::new(reader)),
        Codec::Xz => Box::new(XzDecoder::new_multi_decoder(reader)),
        Codec::Zstd => Box::new(StreamingDecoder::new(reader).map_err(|_| ())?),
    })
}

/// Visits every entry of the archive at `path`.
pub(super) fn for_each_entry(
    path: &Path,
    kind: ArchiveKind,
    visit: &mut dyn FnMut(&EntryInfo, &mut dyn Read) -> Result<Flow, String>,
) -> Result<(), ()> {
    let file = BufReader::new(File::open(path).map_err(|_| ())?);
    match kind {
        ArchiveKind::Zip => archive::for_each_zip_entry(file, visit),
        ArchiveKind::Tar(None) => archive::for_each_tar_entry(file, visit),
        ArchiveKind::Tar(Some(codec)) => archive::for_each_tar_entry(decoder(codec, file)?, visit),
    }
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_archives_and_compressed_files() {
        assert_eq!(archive_kind("app.jar"), Some(ArchiveKind::Zip));
        assert_eq!(archive_kind("backup.tar.zst"), Some(ArchiveKind::Tar(Some(Codec::Zstd))));
        assert_eq!(archive_kind("src.tbz2"), Some(ArchiveKind::Tar(Some(Codec::Bzip2))));
        assert_eq!(archive_kind("notes.txt"), None);
        assert_eq!(compressed("App.Log.GZ"), Some((Codec::Gzip, "App.Log")));
        assert_eq!(compressed("dump.sql.xz"), Some((Codec::Xz, "dump.sql")));
        assert_eq!(compressed(".gz"), None);
    }
}
