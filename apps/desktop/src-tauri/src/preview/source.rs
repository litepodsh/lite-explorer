//! Shared plumbing for previews that parse a file in memory. Routes the read
//! across local files, S3 objects and SFTP/FTP servers so each reader only
//! deals with bytes.

use sqlx::SqlitePool;

use crate::search::text;
use crate::{network, remote};

/// Default cap for in-memory preview reads; keeps huge files out of the parser.
pub(crate) const SOURCE_MAX_BYTES: usize = 32 * 1024 * 1024;

pub(crate) async fn read_bytes(
    pool: &SqlitePool,
    clients: &remote::RemoteClients,
    sessions: &network::servers::Sessions,
    path: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if network::servers::is_server_path(path) {
        return network::servers::read_bytes(pool, sessions, path, max_bytes).await;
    }
    if remote::is_remote_path(path) {
        return remote::read_object(pool, clients, path, max_bytes).await;
    }
    read_local_bytes(path, max_bytes)
}

pub(crate) fn read_local_bytes(path: &str, max_bytes: usize) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > max_bytes as u64 {
        return Err(format!(
            "{} is larger than {} MB",
            std::path::Path::new(path)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string()),
            max_bytes / (1024 * 1024)
        ));
    }
    std::fs::read(path).map_err(|error| error.to_string())
}

/// Decodes a preview payload as text (UTF-8/UTF-16/Windows-1252) or reports that
/// the bytes aren't text.
pub(crate) fn decode_text(bytes: &[u8]) -> Result<String, String> {
    text::decode(bytes)
        .map(|text| text.into_owned())
        .ok_or_else(|| "Not a text file".to_string())
}
