//! Windows: SMB and WebDAV connect to UNC paths with WNet (WebDAV needs the WebClient
//! service); NFS uses `mount.exe` from the "Client for NFS" feature and a drive letter.

use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
    ptr,
};

use windows_sys::Win32::NetworkManagement::WNet::{
    WNetAddConnection2W, WNetCancelConnection2W, NETRESOURCEW, RESOURCETYPE_DISK,
};

use super::super::{ConnectError, ErrorKind, Protocol};
use super::{targets, Credentials, Target};

const ERROR_SESSION_CREDENTIAL_CONFLICT: u32 = 1219;
const ERROR_NOT_CONNECTED: u32 = 2250;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn nfs_tool(name: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join(name)
}

fn run(tool: PathBuf, args: &[&str]) -> Result<std::process::Output, ConnectError> {
    Command::new(tool)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(ConnectError::other)
}

pub fn mount(target: &Target, credentials: &Credentials) -> Result<PathBuf, ConnectError> {
    if target.protocol == Protocol::Nfs {
        return mount_nfs(target);
    }
    let root = targets::unc_root(target)?;
    let local = targets::unc_path(target)?;
    let mut remote = wide(&root);
    let resource = NETRESOURCEW {
        dwScope: 0,
        dwType: RESOURCETYPE_DISK,
        dwDisplayType: 0,
        dwUsage: 0,
        lpLocalName: ptr::null_mut(),
        lpRemoteName: remote.as_mut_ptr(),
        lpComment: ptr::null_mut(),
        lpProvider: ptr::null_mut(),
    };
    let user = credentials.username.as_deref().map(wide);
    let password = credentials.password.as_deref().map(wide);
    let status = unsafe {
        WNetAddConnection2W(
            &resource,
            password
                .as_ref()
                .map_or(ptr::null(), |password| password.as_ptr()),
            user.as_ref().map_or(ptr::null(), |user| user.as_ptr()),
            0,
        )
    };
    // 1219 also means Windows already holds a session to this server; reuse it when the
    // folder is reachable.
    if status != 0 && !(status == ERROR_SESSION_CREDENTIAL_CONFLICT && Path::new(&local).is_dir()) {
        return Err(targets::wnet_error(status));
    }
    Ok(PathBuf::from(local))
}

fn mount_nfs(target: &Target) -> Result<PathBuf, ConnectError> {
    if let Some(local) = find_mounted(target) {
        return Ok(local);
    }
    let tool = nfs_tool("mount.exe");
    if !tool.exists() {
        return Err(ConnectError::new(
            ErrorKind::Unsupported,
            "Turn on “Client for NFS” in Windows Features to connect to NFS shares.",
        ));
    }
    let remote = targets::unc_path(target)?;
    let output = run(tool, &["-o", "anon", &remote, "*"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    targets::windows_nfs_drive(&stdout)
        .map(PathBuf::from)
        .ok_or_else(|| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = stderr
                .lines()
                .chain(stdout.lines())
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("Windows couldn’t mount the NFS share.");
            ConnectError::new(ErrorKind::Other, message)
        })
}

/// Only NFS drives can be listed cheaply. SMB and WebDAV connections are reopened on demand,
/// because probing a UNC path can hang for a long time when the server is offline.
pub fn find_mounted(target: &Target) -> Option<PathBuf> {
    if target.protocol != Protocol::Nfs {
        return None;
    }
    let tool = nfs_tool("mount.exe");
    if !tool.exists() {
        return None;
    }
    let output = run(tool, &[]).ok()?;
    let remote = targets::unc_path(target).ok()?;
    targets::windows_nfs_listed(&String::from_utf8_lossy(&output.stdout), &remote)
        .map(PathBuf::from)
}

pub fn unmount(target: &Target, local: &Path) -> Result<(), ConnectError> {
    if target.protocol == Protocol::Nfs {
        let drive = local.to_string_lossy();
        let drive = drive.trim_end_matches('\\');
        let output = run(nfs_tool("umount.exe"), &["-f", drive])?;
        return if output.status.success() {
            Ok(())
        } else {
            Err(ConnectError::other(
                String::from_utf8_lossy(&output.stdout).trim(),
            ))
        };
    }
    let root = wide(&targets::unc_root(target)?);
    let status = unsafe { WNetCancelConnection2W(root.as_ptr(), 0, 1) };
    if status == 0 || status == ERROR_NOT_CONNECTED {
        Ok(())
    } else {
        Err(targets::wnet_error(status))
    }
}
