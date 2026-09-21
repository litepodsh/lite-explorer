//! Mounting network shares with the operating system: NetFS on macOS, GIO (GVfs) on Linux,
//! WNet and the Client for NFS on Windows. A mounted share is a plain local path, so
//! listing, preview and copy go through the local file code. All functions block.

use std::path::{Path, PathBuf};

#[allow(unused_imports)]
use super::{ConnectError, ErrorKind, Protocol, Security};

// Each platform uses only its own part of these helpers.
#[allow(dead_code)]
mod targets;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as platform;

/// Share to mount. `path` is normalized: `share/sub` for SMB, `/absolute` otherwise.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub protocol: Protocol,
    pub host: String,
    pub port: Option<u16>,
    pub path: String,
    pub security: Option<Security>,
}

/// Account used to mount. No username means guest (or anonymous) access.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

pub fn is_mountable(protocol: Protocol) -> bool {
    matches!(protocol, Protocol::Smb | Protocol::Nfs | Protocol::Webdav)
}

/// Whether this system can browse an SMB server without naming a share first.
pub fn picks_shares() -> bool {
    cfg!(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))
}

/// SMB needs a share name only on systems that cannot browse a server root.
pub fn require_share(target: &Target) -> Result<(), ConnectError> {
    if target.protocol == Protocol::Smb
        && targets::split_share(&target.path).0.is_empty()
        && !picks_shares()
    {
        return Err(ConnectError::invalid("Enter a share name."));
    }
    Ok(())
}

/// SMB paths are `share/sub/folders`. Returns the share and the rest.
pub fn targets_split_share(path: &str) -> (&str, &str) {
    targets::split_share(path)
}

/// An SMB location without a share stands for the whole server.
pub fn is_server_root(target: &Target) -> bool {
    target.protocol == Protocol::Smb && targets::split_share(&target.path).0.is_empty()
}

/// SMB shares of the target's server that are mounted now, as share name and local folder.
#[cfg(target_os = "macos")]
pub fn mounted_shares(target: &Target) -> Vec<(String, PathBuf)> {
    platform::mounted_shares(target)
}

/// Disk shares of the target's server that the account can open.
#[cfg(target_os = "macos")]
pub fn list_shares(
    target: &Target,
    credentials: &Credentials,
) -> Result<Vec<String>, ConnectError> {
    platform::list_shares(target, credentials)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
pub fn list_shares(
    target: &Target,
    credentials: &Credentials,
) -> Result<Vec<String>, ConnectError> {
    platform::list_shares(target, credentials)
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub fn list_shares(
    _target: &Target,
    _credentials: &Credentials,
) -> Result<Vec<String>, ConnectError> {
    Err(unsupported_system())
}

/// Non-desktop systems cannot enumerate mounted SMB shares.
#[cfg(not(target_os = "macos"))]
pub fn mounted_shares(_target: &Target) -> Vec<(String, PathBuf)> {
    Vec::new()
}

/// Mounts the target, or reuses an existing mount, and returns the local folder of `target.path`.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn mount(target: &Target, credentials: &Credentials) -> Result<PathBuf, ConnectError> {
    require_share(target)?;
    platform::mount(target, credentials)
}

/// Local folder of `target.path` when the share is already mounted.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn find_mounted(target: &Target) -> Option<PathBuf> {
    platform::find_mounted(target)
}

/// Unmounts the share that contains `local`.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
pub fn unmount(target: &Target, local: &Path) -> Result<(), ConnectError> {
    platform::unmount(target, local)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn mount(_target: &Target, _credentials: &Credentials) -> Result<PathBuf, ConnectError> {
    Err(unsupported_system())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn find_mounted(_target: &Target) -> Option<PathBuf> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn unmount(_target: &Target, _local: &Path) -> Result<(), ConnectError> {
    Err(unsupported_system())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn unsupported_system() -> ConnectError {
    ConnectError::new(
        ErrorKind::Unsupported,
        "Mounting network shares isn’t supported on this system.",
    )
}
