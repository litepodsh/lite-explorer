//! Containers content search opens. Detection lives in `explorer::archive` so the
//! archive preview and content search agree on every format.

use std::io::Read;
use std::path::Path;

use crate::explorer::archive::{EntryInfo, Flow};

pub(super) use crate::explorer::archive::{
    archive_kind, compressed, decoder, for_each_archive_entry, ArchiveKind, Codec,
};

/// Visits every entry of the archive at `path`, mapping errors to the unit search uses.
pub(super) fn for_each_entry(
    path: &Path,
    kind: ArchiveKind,
    visit: &mut dyn FnMut(&EntryInfo, &mut dyn Read) -> Result<Flow, String>,
) -> Result<(), ()> {
    for_each_archive_entry(path, kind, visit).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_archives_and_compressed_files() {
        assert_eq!(archive_kind("app.jar"), Some(ArchiveKind::Zip));
        assert_eq!(
            archive_kind("backup.tar.zst"),
            Some(ArchiveKind::Tar(Some(Codec::Zstd)))
        );
        assert_eq!(
            archive_kind("src.tbz2"),
            Some(ArchiveKind::Tar(Some(Codec::Bzip2)))
        );
        assert_eq!(archive_kind("notes.txt"), None);
        assert_eq!(compressed("App.Log.GZ"), Some((Codec::Gzip, "App.Log")));
        assert_eq!(compressed("dump.sql.xz"), Some((Codec::Xz, "dump.sql")));
        assert_eq!(compressed(".gz"), None);
    }
}
