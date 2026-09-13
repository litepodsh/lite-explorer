//! macOS: NetFS mounts shares under /Volumes, like Finder's "Connect to Server".

use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    ptr,
};

use core_foundation_sys::{
    array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef},
    base::{CFRelease, CFTypeRef},
    dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreateMutable,
        CFDictionarySetValue, CFMutableDictionaryRef,
    },
    number::kCFBooleanTrue,
    string::{
        kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringGetLength,
        CFStringRef,
    },
    url::{CFURLCreateWithString, CFURLRef},
};

use super::super::{ConnectError, ErrorKind, Protocol};
use super::{targets, Credentials, Target};

const EEXIST: i32 = 17;

#[link(name = "NetFS", kind = "framework")]
extern "C" {
    fn NetFSMountURLSync(
        url: CFURLRef,
        mountpath: CFURLRef,
        user: CFStringRef,
        passwd: CFStringRef,
        open_options: CFMutableDictionaryRef,
        mount_options: CFMutableDictionaryRef,
        mountpoints: *mut CFArrayRef,
    ) -> i32;
}

/// Releases a Core Foundation object when dropped.
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) };
        }
    }
}

fn cf_string(value: &str) -> Owned {
    let value = CString::new(value.replace('\0', "")).unwrap_or_default();
    Owned(unsafe {
        CFStringCreateWithCString(ptr::null(), value.as_ptr(), kCFStringEncodingUTF8) as CFTypeRef
    })
}

unsafe fn string_from_cf(string: CFStringRef) -> Option<String> {
    if string.is_null() {
        return None;
    }
    let capacity = CFStringGetLength(string) * 4 + 1;
    let mut buffer = vec![0u8; capacity as usize];
    let ok = CFStringGetCString(
        string,
        buffer.as_mut_ptr().cast(),
        capacity,
        kCFStringEncodingUTF8,
    );
    (ok != 0).then(|| {
        CStr::from_bytes_until_nul(&buffer)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

fn dictionary() -> Owned {
    Owned(unsafe {
        CFDictionaryCreateMutable(
            ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        ) as CFTypeRef
    })
}

fn set(dictionary: &Owned, key: &str, value: CFTypeRef) {
    let key = cf_string(key);
    unsafe { CFDictionarySetValue(dictionary.0 as CFMutableDictionaryRef, key.0, value) };
}

pub fn mount(target: &Target, credentials: &Credentials) -> Result<PathBuf, ConnectError> {
    if let Some(local) = find_mounted(target) {
        return Ok(local);
    }
    let url_string = cf_string(&targets::netfs_url(target));
    let url = Owned(unsafe {
        CFURLCreateWithString(ptr::null(), url_string.0 as CFStringRef, ptr::null()) as CFTypeRef
    });
    if url.0.is_null() {
        return Err(ConnectError::invalid("The server address isn’t valid."));
    }

    // An SMB location without a share lets macOS show its share picker. Otherwise no UI,
    // so a wrong password comes back as an error instead of a system dialog.
    let pick_share = target.protocol == Protocol::Smb && target.path.is_empty();
    let open_options = dictionary();
    let ui = cf_string(if pick_share { "AllowUI" } else { "NoUI" });
    set(&open_options, "UIOption", ui.0);
    if credentials.username.is_none() {
        set(&open_options, "Guest", unsafe { kCFBooleanTrue }
            as CFTypeRef);
    }
    let mount_options = dictionary();
    set(&mount_options, "AllowSubMounts", unsafe { kCFBooleanTrue }
        as CFTypeRef);
    set(&mount_options, "SoftMount", unsafe { kCFBooleanTrue }
        as CFTypeRef);

    let user = credentials.username.as_deref().map(cf_string);
    let password = credentials.password.as_deref().map(cf_string);
    let mut mountpoints: CFArrayRef = ptr::null();
    let status = unsafe {
        NetFSMountURLSync(
            url.0 as CFURLRef,
            ptr::null(),
            user.as_ref()
                .map_or(ptr::null(), |user| user.0 as CFStringRef),
            password
                .as_ref()
                .map_or(ptr::null(), |password| password.0 as CFStringRef),
            open_options.0 as CFMutableDictionaryRef,
            mount_options.0 as CFMutableDictionaryRef,
            &mut mountpoints,
        )
    };
    let mountpoints = Owned(mountpoints as CFTypeRef);

    if status == EEXIST {
        return find_mounted(target).ok_or_else(|| targets::netfs_error(status));
    }
    if status != 0 {
        return Err(targets::netfs_error(status));
    }
    let first = unsafe {
        let array = mountpoints.0 as CFArrayRef;
        if array.is_null() || CFArrayGetCount(array) == 0 {
            None
        } else {
            string_from_cf(CFArrayGetValueAtIndex(array, 0) as CFStringRef)
        }
    };
    first
        .map(PathBuf::from)
        .or_else(|| find_mounted(target))
        .ok_or_else(|| {
            ConnectError::new(
                ErrorKind::Other,
                "macOS mounted the share but didn’t say where.",
            )
        })
}

fn c_field(field: &[libc::c_char]) -> String {
    unsafe { CStr::from_ptr(field.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

pub fn find_mounted(target: &Target) -> Option<PathBuf> {
    let mut mounts: *mut libc::statfs = ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mounts, libc::MNT_NOWAIT) };
    if count <= 0 || mounts.is_null() {
        return None;
    }
    // getmntinfo owns this buffer; it must not be freed.
    let mounts = unsafe { std::slice::from_raw_parts(mounts, count as usize) };
    mounts.iter().find_map(|entry| {
        let rest = targets::match_mount(
            &c_field(&entry.f_fstypename),
            &c_field(&entry.f_mntfromname),
            target,
        )?;
        let root = PathBuf::from(c_field(&entry.f_mntonname));
        Some(if rest.is_empty() {
            root
        } else {
            root.join(rest)
        })
    })
}

/// Disk shares the account can see, from `smbutil view`. The password goes through stdin in
/// a new session without a terminal, so it never appears in process arguments.
pub fn list_shares(
    target: &Target,
    credentials: &Credentials,
) -> Result<Vec<String>, ConnectError> {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut command = Command::new("/usr/bin/smbutil");
    command.arg("view");
    if credentials.username.is_none() {
        command.arg("-g");
    }
    command
        .arg(targets::smbutil_address(
            target,
            credentials.username.as_deref(),
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Without a controlling terminal, smbutil reads the password prompt answer from stdin.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(ConnectError::other)?;
    if let Some(mut stdin) = child.stdin.take() {
        let password = credentials.password.clone().unwrap_or_default();
        let _ = writeln!(stdin, "{password}");
    }
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(ConnectError::other)?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ConnectError::new(
                ErrorKind::Timeout,
                "The server didn’t answer in time.",
            ));
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let output = child.wait_with_output().map_err(ConnectError::other)?;
    if !output.status.success() {
        return Err(targets::smbutil_error(&String::from_utf8_lossy(
            &output.stderr,
        )));
    }
    Ok(targets::smbutil_shares(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

pub fn mounted_shares(target: &Target) -> Vec<(String, PathBuf)> {
    let mut mounts: *mut libc::statfs = ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mounts, libc::MNT_NOWAIT) };
    if count <= 0 || mounts.is_null() {
        return Vec::new();
    }
    // getmntinfo owns this buffer; it must not be freed.
    let mounts = unsafe { std::slice::from_raw_parts(mounts, count as usize) };
    mounts
        .iter()
        .filter_map(|entry| {
            let share = targets::smb_mount_share(
                &c_field(&entry.f_fstypename),
                &c_field(&entry.f_mntfromname),
                &target.host,
            )?;
            Some((share, PathBuf::from(c_field(&entry.f_mntonname))))
        })
        .collect()
}

pub fn unmount(_target: &Target, local: &Path) -> Result<(), ConnectError> {
    let path = CString::new(local.to_string_lossy().as_bytes()).map_err(ConnectError::other)?;
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(path.as_ptr(), &mut stats) } != 0 {
        // Already gone.
        return Ok(());
    }
    let root = CString::new(c_field(&stats.f_mntonname)).map_err(ConnectError::other)?;
    if unsafe { libc::unmount(root.as_ptr(), 0) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    Err(match error.raw_os_error() {
        Some(libc::EBUSY) => ConnectError::new(
            ErrorKind::Other,
            "The share is in use. Close files and apps using it, then try again.",
        ),
        _ => ConnectError::other(format!("Couldn’t disconnect: {error}.")),
    })
}
