//! Linux: GVfs mounts shares through `gio mount`, and exposes them as local folders under
//! `/run/user/<uid>/gvfs` when gvfs-fuse is installed.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use super::super::{ConnectError, ErrorKind};
use super::{targets, Credentials, Target};

fn gio_missing(error: std::io::Error) -> ConnectError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ConnectError::new(
            ErrorKind::Unsupported,
            "Install GVfs (the gio command) to connect to network shares.",
        )
    } else {
        ConnectError::other(error)
    }
}

pub fn mount(target: &Target, credentials: &Credentials) -> Result<PathBuf, ConnectError> {
    if let Some(local) = find_mounted(target) {
        return Ok(local);
    }
    let uri = targets::gio_uri(target);
    let mut command = Command::new("gio");
    command.arg("mount");
    if credentials.username.is_none() {
        command.arg("--anonymous");
    }
    let mut child = command
        .arg(&uri)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(gio_missing)?;
    // gio reads prompt answers (user, domain, password) from stdin, which keeps the
    // password out of the process arguments.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(targets::gio_answers(target.protocol, credentials).as_bytes());
    }
    let output = child.wait_with_output().map_err(ConnectError::other)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !targets::gio_already_mounted(&stderr) {
            return Err(targets::gio_error(&stderr));
        }
    }
    find_mounted(target).ok_or_else(|| {
        ConnectError::new(
            ErrorKind::Unsupported,
            "The share connected, but has no local folder. Install gvfs-fuse and try again.",
        )
    })
}

pub fn find_mounted(target: &Target) -> Option<PathBuf> {
    let output = Command::new("gio")
        .arg("info")
        .arg(targets::gio_uri(target))
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    targets::gio_local_path(&String::from_utf8_lossy(&output.stdout)).map(PathBuf::from)
}

pub fn unmount(target: &Target, _local: &Path) -> Result<(), ConnectError> {
    let output = Command::new("gio")
        .args(["mount", "--unmount"])
        .arg(targets::gio_uri(target))
        .stdin(Stdio::null())
        .output()
        .map_err(gio_missing)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(targets::gio_error(&String::from_utf8_lossy(&output.stderr)))
    }
}
