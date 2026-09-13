//! SFTP over SSH with russh. Host keys are pinned on first use (see `known_hosts`).

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use russh::{
    client,
    keys::{HashAlg, PublicKeyOrCertificate},
    Disconnect,
};
use russh_sftp::{
    client::{error::Error as SftpError, SftpSession},
    protocol::StatusCode,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::{ConnectError, ErrorKind};
use super::{join, lost, untrusted, Progress, RemoteEntry, ServerTarget, CHUNK, CONNECT_TIMEOUT};

pub struct SftpConnection {
    handle: client::Handle<HostKeyCheck>,
    sftp: SftpSession,
}

/// Accepts the server only when its key matches the pinned fingerprint, and records the
/// key it saw so an unknown or changed key can be shown to the user.
struct HostKeyCheck {
    known: Option<String>,
    seen: Arc<Mutex<Option<(String, String)>>>,
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = server_public_key.public_key();
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let trusted = self.known.as_deref() == Some(fingerprint.as_str());
        *self.seen.lock().unwrap() = Some((key.algorithm().to_string(), fingerprint));
        Ok(trusted)
    }
}

fn russh_error(error: russh::Error) -> ConnectError {
    match error {
        russh::Error::IO(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
            ConnectError::new(ErrorKind::Unreachable, "The server refused the connection.")
        }
        russh::Error::IO(error) => ConnectError::new(
            ErrorKind::Unreachable,
            format!("Can’t reach the server: {error}."),
        ),
        russh::Error::ConnectionTimeout
        | russh::Error::KeepaliveTimeout
        | russh::Error::InactivityTimeout => {
            ConnectError::new(ErrorKind::Timeout, "The server didn’t answer in time.")
        }
        russh::Error::Disconnect | russh::Error::HUP | russh::Error::SendError => lost(),
        error => ConnectError::other(error),
    }
}

fn sftp_error(error: SftpError) -> ConnectError {
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => ConnectError::new(
                ErrorKind::NotFound,
                "The file or folder doesn’t exist on the server.",
            ),
            StatusCode::PermissionDenied => ConnectError::new(
                ErrorKind::Other,
                "The server denied permission for this item.",
            ),
            StatusCode::NoConnection | StatusCode::ConnectionLost => lost(),
            _ if !status.error_message.is_empty() => ConnectError::other(status.error_message),
            code => ConnectError::other(code),
        },
        SftpError::IO(_) | SftpError::Timeout => lost(),
        error => ConnectError::other(error),
    }
}

pub async fn connect(
    target: &ServerTarget,
    username: &str,
    password: &str,
    known: Option<String>,
) -> Result<SftpConnection, ConnectError> {
    let seen = Arc::new(Mutex::new(None));
    let check = HostKeyCheck {
        known: known.clone(),
        seen: seen.clone(),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(15 * 60)),
        keepalive_interval: Some(Duration::from_secs(30)),
        ..Default::default()
    });
    let connecting = client::connect(config, (target.host.as_str(), target.port), check);
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
        Err(_) => {
            return Err(ConnectError::new(
                ErrorKind::Timeout,
                "The server didn’t answer in time.",
            ))
        }
        Ok(Ok(handle)) => handle,
        Ok(Err(error)) => {
            let presented = seen.lock().unwrap().take();
            if let Some((algorithm, fingerprint)) = presented {
                if known.as_deref() != Some(fingerprint.as_str()) {
                    return Err(untrusted(target, algorithm, fingerprint, known.is_some()));
                }
            }
            return Err(russh_error(error));
        }
    };

    let auth = handle
        .authenticate_password(username, password)
        .await
        .map_err(russh_error)?;
    if !auth.success() {
        return Err(ConnectError::new(
            ErrorKind::Auth,
            "The server rejected the username or password.",
        ));
    }
    let channel = handle.channel_open_session().await.map_err(russh_error)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(russh_error)?;
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(sftp_error)?;
    Ok(SftpConnection { handle, sftp })
}

impl SftpConnection {
    pub async fn home(&self) -> Result<String, ConnectError> {
        self.sftp.canonicalize(".").await.map_err(sftp_error)
    }

    pub async fn list(&self, dir: &str) -> Result<Vec<RemoteEntry>, ConnectError> {
        let mut entries = Vec::new();
        for entry in self.sftp.read_dir(dir).await.map_err(sftp_error)? {
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let metadata = entry.metadata();
            let mut is_dir = entry.file_type().is_dir();
            if entry.file_type().is_symlink() {
                if let Ok(resolved) = self.sftp.metadata(join(dir, &name)).await {
                    is_dir = resolved.file_type().is_dir();
                }
            }
            entries.push(RemoteEntry {
                name,
                is_dir,
                size: metadata.size.unwrap_or(0),
                modified: metadata.mtime.map(|seconds| u64::from(seconds) * 1000),
            });
        }
        Ok(entries)
    }

    pub async fn stat(&self, path: &str) -> Result<RemoteEntry, ConnectError> {
        let metadata = self.sftp.metadata(path).await.map_err(sftp_error)?;
        Ok(RemoteEntry {
            name: super::file_name(path).to_string(),
            is_dir: metadata.file_type().is_dir(),
            size: metadata.size.unwrap_or(0),
            modified: metadata.mtime.map(|seconds| u64::from(seconds) * 1000),
        })
    }

    pub async fn mkdir(&self, path: &str) -> Result<(), ConnectError> {
        self.sftp.create_dir(path).await.map_err(sftp_error)
    }

    pub async fn create_file(&self, path: &str) -> Result<(), ConnectError> {
        let mut file = self.sftp.create(path).await.map_err(sftp_error)?;
        file.shutdown().await.map_err(|_| lost())
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), ConnectError> {
        self.sftp.rename(from, to).await.map_err(sftp_error)
    }

    pub async fn remove_file(&self, path: &str) -> Result<(), ConnectError> {
        self.sftp.remove_file(path).await.map_err(sftp_error)
    }

    pub async fn remove_dir(&self, path: &str) -> Result<(), ConnectError> {
        self.sftp.remove_dir(path).await.map_err(sftp_error)
    }

    pub async fn read_head(&self, path: &str, max: usize) -> Result<Vec<u8>, ConnectError> {
        let file = self.sftp.open(path).await.map_err(sftp_error)?;
        let mut bytes = Vec::new();
        file.take(max as u64)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| lost())?;
        Ok(bytes)
    }

    pub async fn download(
        &self,
        path: &str,
        local: &Path,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        let mut remote = self.sftp.open(path).await.map_err(sftp_error)?;
        let mut output = tokio::fs::File::create(local)
            .await
            .map_err(ConnectError::other)?;
        let mut buffer = vec![0u8; CHUNK];
        loop {
            if progress.cancelled() {
                return Err(ConnectError::other("Canceled."));
            }
            let read = remote.read(&mut buffer).await.map_err(|_| lost())?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .await
                .map_err(ConnectError::other)?;
            progress.add_bytes(read as u64);
        }
        output.flush().await.map_err(ConnectError::other)
    }

    pub async fn upload(
        &self,
        local: &Path,
        path: &str,
        progress: &mut Progress,
    ) -> Result<(), ConnectError> {
        let mut input = tokio::fs::File::open(local)
            .await
            .map_err(ConnectError::other)?;
        let mut remote = self.sftp.create(path).await.map_err(sftp_error)?;
        let mut buffer = vec![0u8; CHUNK];
        loop {
            if progress.cancelled() {
                return Err(ConnectError::other("Canceled."));
            }
            let read = input.read(&mut buffer).await.map_err(ConnectError::other)?;
            if read == 0 {
                break;
            }
            remote
                .write_all(&buffer[..read])
                .await
                .map_err(|_| lost())?;
            progress.add_bytes(read as u64);
        }
        remote.shutdown().await.map_err(|_| lost())
    }

    pub async fn close(&self) {
        let _ = self.sftp.close().await;
        let _ = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }
}
