use std::{collections::HashSet, fs, path::Path, process::Command};

use serde::Serialize;

use crate::explorer::paths::{home_location, startup_volume_name};

#[derive(Serialize)]
pub struct DeviceInfo {
    name: String,
    chip: Option<String>,
    cores: usize,
    memory_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct VolumeInfo {
    pub(crate) name: String,
    pub(crate) mount_point: String,
    file_system: Option<String>,
    total_bytes: u64,
    free_bytes: u64,
    is_primary: bool,
}

fn is_hfs_external(volume: &VolumeInfo) -> bool {
    !volume.is_primary && volume.file_system.as_deref() == Some("HFS")
}

/// HFS volumes that macOS mounted under `/Volumes`, excluding the startup disk.
pub(crate) fn hfs_external_volumes() -> Vec<VolumeInfo> {
    volumes().into_iter().filter(is_hfs_external).collect()
}

#[tauri::command]
pub fn eject_volume(path: String) -> Result<(), String> {
    if !hfs_external_volumes()
        .iter()
        .any(|volume| volume.mount_point == path)
    {
        return Err("That mounted HFS volume is no longer available.".into());
    }

    #[cfg(target_os = "macos")]
    {
        let output = Command::new("diskutil")
            .args(["eject", &path])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }

    #[cfg(not(target_os = "macos"))]
    Err("Ejecting HFS volumes is available only on macOS.".into())
}

#[derive(Serialize)]
pub struct DiskOverview {
    device: DeviceInfo,
    volumes: Vec<VolumeInfo>,
    home: Option<String>,
}

pub struct VolumeStats {
    total_bytes: u64,
    free_bytes: u64,
    file_system: Option<String>,
    mounted_on: Option<String>,
}

#[cfg(target_os = "macos")]
pub fn volume_stats(path: &Path) -> Option<VolumeStats> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block_size = stats.f_bsize as u64;
    let file_system = unsafe { CStr::from_ptr(stats.f_fstypename.as_ptr()) };
    let mounted_on = unsafe { CStr::from_ptr(stats.f_mntonname.as_ptr()) };
    Some(VolumeStats {
        total_bytes: stats.f_blocks as u64 * block_size,
        free_bytes: stats.f_bavail as u64 * block_size,
        file_system: Some(file_system.to_string_lossy().to_uppercase()),
        mounted_on: Some(mounted_on.to_string_lossy().into_owned()),
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn volume_stats(path: &Path) -> Option<VolumeStats> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block_size = stats.f_frsize as u64;
    Some(VolumeStats {
        total_bytes: stats.f_blocks as u64 * block_size,
        free_bytes: stats.f_bavail as u64 * block_size,
        file_system: None,
        mounted_on: None,
    })
}

#[cfg(target_os = "windows")]
pub fn volume_stats(path: &Path) -> Option<VolumeStats> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) } == 0 {
        return None;
    }
    Some(VolumeStats {
        total_bytes: total,
        free_bytes: available,
        file_system: None,
        mounted_on: None,
    })
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn volume_stats(_path: &Path) -> Option<VolumeStats> {
    None
}

/// Drive letters present in a `GetLogicalDrives` bit mask, where bit 0 is `A:`.
#[cfg(any(target_os = "windows", test))]
pub fn drive_letters(mask: u32) -> Vec<char> {
    (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| (b'A' + bit) as char)
        .collect()
}

/// File Explorer's naming: the volume label, or "Local Disk" when the drive has none.
#[cfg(any(target_os = "windows", test))]
pub fn drive_display_name(label: &str, letter: char) -> String {
    let label = label.trim();
    let label = if label.is_empty() {
        "Local Disk"
    } else {
        label
    };
    format!("{label} ({letter}:)")
}

#[cfg(target_os = "windows")]
pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

#[cfg(target_os = "windows")]
pub fn from_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Docker Desktop registers its own distros; they hold no user files.
#[cfg(any(target_os = "windows", test))]
pub fn is_user_wsl_distro(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !name.starts_with("docker-desktop")
}

/// `\\wsl$` works on every WSL-capable build; `\\wsl.localhost` needs Windows 10 21H2+.
#[cfg(any(target_os = "windows", test))]
pub fn wsl_path(distro: &str) -> String {
    format!(r"\\wsl$\{}\", distro.trim())
}

/// WSL distros registered for the current user. Read from the registry because touching
/// `\\wsl$\<distro>` boots a stopped distro, and locations are polled while the window is open.
#[cfg(target_os = "windows")]
pub fn wsl_distros() -> Vec<String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
        RRF_RT_REG_SZ,
    };
    let lxss = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Lxss");
    let mut key: HKEY = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, lxss.as_ptr(), 0, KEY_READ, &mut key) } != 0 {
        return Vec::new();
    }
    let value = to_wide("DistributionName");
    let mut distros = Vec::new();
    for index in 0.. {
        let mut id = [0u16; 128];
        let mut id_len = id.len() as u32;
        let status = unsafe {
            RegEnumKeyExW(
                key,
                index,
                id.as_mut_ptr(),
                &mut id_len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            break;
        }
        let mut name = [0u16; 256];
        let mut size = (name.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                key,
                id.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                name.as_mut_ptr().cast(),
                &mut size,
            )
        };
        let name = from_wide(&name);
        if status == 0 && is_user_wsl_distro(&name) {
            distros.push(name.trim().to_owned());
        }
    }
    unsafe { RegCloseKey(key) };
    distros.sort_by_key(|name| name.to_lowercase());
    distros
}

#[cfg(target_os = "macos")]
pub fn volumes() -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    if let Some(stats) = volume_stats(Path::new("/")) {
        volumes.push(VolumeInfo {
            name: startup_volume_name(),
            mount_point: "/".into(),
            file_system: stats.file_system,
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary: true,
        });
    }
    let Ok(read) = fs::read_dir("/Volumes") else {
        return volumes;
    };
    for entry in read.filter_map(Result::ok) {
        let path = entry.path();
        let mount_point = path.to_string_lossy().into_owned();
        let Some(stats) = volume_stats(&path) else {
            continue;
        };
        // "/Volumes/Macintosh HD" is a symlink to "/", and plain folders are not mounts.
        if stats.mounted_on.as_deref() != Some(mount_point.as_str()) || stats.total_bytes == 0 {
            continue;
        }
        volumes.push(VolumeInfo {
            name: entry.file_name().to_string_lossy().into_owned(),
            mount_point,
            file_system: stats.file_system,
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary: false,
        });
    }
    volumes
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn volumes() -> Vec<VolumeInfo> {
    let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut volumes = Vec::new();
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let (Some(_device), Some(mount_point), Some(file_system)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let mount_point = mount_point.replace("\\040", " ");
        let is_primary = mount_point == "/";
        let is_removable = ["/media/", "/mnt/", "/run/media/"]
            .iter()
            .any(|prefix| mount_point.starts_with(prefix));
        if !is_primary && !is_removable {
            continue;
        }
        let Some(stats) = volume_stats(Path::new(&mount_point)) else {
            continue;
        };
        if stats.total_bytes == 0 {
            continue;
        }
        volumes.push(VolumeInfo {
            name: if is_primary {
                startup_volume_name()
            } else {
                Path::new(&mount_point)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| mount_point.clone())
            },
            mount_point,
            file_system: Some(file_system.to_uppercase()),
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary,
        });
    }
    volumes.sort_by_key(|volume| !volume.is_primary);
    volumes
}

/// Fixed and removable drives with media, the system drive first.
#[cfg(target_os = "windows")]
pub fn volumes() -> Vec<VolumeInfo> {
    use std::time::Duration;
    use windows_sys::Win32::{
        Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives},
        System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE},
    };

    fn probe_volume_blocking(root: String, letter: char, system_drive: char) -> Option<VolumeInfo> {
        use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
        let root_wide = to_wide(&root);
        let stats = volume_stats(Path::new(&root))?;
        if stats.total_bytes == 0 {
            return None;
        }
        let mut label = [0u16; 261];
        let mut file_system = [0u16; 261];
        let has_info = unsafe {
            GetVolumeInformationW(
                root_wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                file_system.as_mut_ptr(),
                file_system.len() as u32,
            )
        } != 0;
        Some(VolumeInfo {
            name: drive_display_name(
                &if has_info {
                    from_wide(&label)
                } else {
                    String::new()
                },
                letter,
            ),
            mount_point: root,
            file_system: has_info
                .then(|| from_wide(&file_system))
                .filter(|name| !name.is_empty()),
            total_bytes: stats.total_bytes,
            free_bytes: stats.free_bytes,
            is_primary: letter == system_drive,
        })
    }

    // A no-media card reader or floppy can hang GetDiskFreeSpaceExW / GetVolumeInformationW for a
    // long time. Probe each fixed/removable drive on its own thread and give up quickly.
    fn probe_volume(root: String, letter: char, system_drive: char) -> Option<VolumeInfo> {
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(probe_volume_blocking(root, letter, system_drive));
        });
        receiver
            .recv_timeout(Duration::from_millis(500))
            .ok()
            .flatten()
    }

    let system_drive = std::env::var("SystemDrive")
        .ok()
        .and_then(|drive| drive.chars().next())
        .unwrap_or('C')
        .to_ascii_uppercase();
    let mut volumes = Vec::new();
    for letter in drive_letters(unsafe { GetLogicalDrives() }) {
        let root = format!("{letter}:\\");
        let root_wide = to_wide(&root);
        let drive_type = unsafe { GetDriveTypeW(root_wide.as_ptr()) };
        if drive_type != DRIVE_FIXED && drive_type != DRIVE_REMOVABLE {
            continue;
        }
        if let Some(volume) = probe_volume(root, letter, system_drive) {
            volumes.push(volume);
        }
    }
    volumes.sort_by_key(|volume| !volume.is_primary);
    volumes
}

#[cfg(not(any(unix, target_os = "windows")))]
pub fn volumes() -> Vec<VolumeInfo> {
    Vec::new()
}

#[cfg(target_os = "macos")]
pub fn sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut length = 0usize;
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || length == 0 {
        return None;
    }
    let mut buffer = vec![0u8; length];
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    buffer.truncate(length);
    std::ffi::CStr::from_bytes_until_nul(&buffer)
        .ok()
        .map(|value| value.to_string_lossy().trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(target_os = "macos")]
pub fn sysctl_u64(name: &std::ffi::CStr) -> Option<u64> {
    let mut value = 0u64;
    let mut length = std::mem::size_of::<u64>();
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && length == std::mem::size_of::<u64>()).then_some(value)
}

#[cfg(target_os = "macos")]
pub fn device_info() -> DeviceInfo {
    let name = Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| sysctl_string(c"kern.hostname"))
        .unwrap_or_else(|| "This Mac".into());
    DeviceInfo {
        name,
        chip: sysctl_string(c"machdep.cpu.brand_string"),
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
        memory_bytes: sysctl_u64(c"hw.memsize"),
    }
}

#[cfg(target_os = "windows")]
pub fn device_info() -> DeviceInfo {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let name = std::env::var("COMPUTERNAME")
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This PC".into());
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    let memory_bytes =
        (unsafe { GlobalMemoryStatusEx(&mut memory) } != 0).then_some(memory.ullTotalPhys);
    DeviceInfo {
        name,
        chip: windows_processor_name(),
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
        memory_bytes,
    }
}

/// The marketing CPU name ("AMD Ryzen 7 7840U …"), which Windows keeps in the registry.
#[cfg(target_os = "windows")]
pub fn windows_processor_name() -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let key = to_wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = to_wide("ProcessorNameString");
    let mut buffer = [0u16; 256];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    (status == 0)
        .then(|| from_wide(&buffer).trim().to_owned())
        .filter(|name| !name.is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn device_info() -> DeviceInfo {
    let name = fs::read_to_string("/etc/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "This computer".into());
    let chip = fs::read_to_string("/proc/cpuinfo").ok().and_then(|info| {
        info.lines().find_map(|line| {
            line.strip_prefix("model name")
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, model)| model.trim().to_owned())
        })
    });
    let memory_bytes = fs::read_to_string("/proc/meminfo").ok().and_then(|info| {
        info.lines().find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|rest| {
                    rest.trim()
                        .trim_end_matches("kB")
                        .trim()
                        .parse::<u64>()
                        .ok()
                })
                .map(|kilobytes| kilobytes * 1024)
        })
    });
    DeviceInfo {
        name,
        chip,
        cores: std::thread::available_parallelism().map_or(1, |cores| cores.get()),
        memory_bytes,
    }
}

#[tauri::command]
pub async fn disk_overview() -> Result<DiskOverview, String> {
    tauri::async_runtime::spawn_blocking(|| DiskOverview {
        device: device_info(),
        volumes: volumes(),
        home: home_location().map(|home| home.path),
    })
    .await
    .map_err(|error| error.to_string())
}

/// Bytes a file really takes on disk (like `du`), counting hard-linked files once.
#[cfg(unix)]
pub fn allocated_bytes(metadata: &fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> u64 {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_dir() && metadata.nlink() > 1 && !seen.insert((metadata.dev(), metadata.ino()))
    {
        return 0;
    }
    metadata.blocks() * 512
}

#[cfg(not(unix))]
pub fn allocated_bytes(metadata: &fs::Metadata, _seen: &mut HashSet<(u64, u64)>) -> u64 {
    metadata.len()
}

#[cfg(unix)]
pub fn device_id(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.dev())
}

#[cfg(not(unix))]
pub fn device_id(_metadata: &fs::Metadata) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_non_primary_hfs_volumes_are_external() {
        let external = VolumeInfo {
            name: "Gitru".into(),
            mount_point: "/Volumes/Gitru".into(),
            file_system: Some("HFS".into()),
            total_bytes: 1,
            free_bytes: 1,
            is_primary: false,
        };
        assert!(is_hfs_external(&external));
        assert!(!is_hfs_external(&VolumeInfo {
            is_primary: true,
            ..external
        }));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_disk_overview_returns_the_system_drive() {
        let device = device_info();
        let volumes = volumes();
        assert!(!device.name.is_empty());
        assert!(volumes.iter().any(|volume| volume.is_primary));
    }

    #[test]
    fn drive_letters_follow_the_logical_drives_mask() {
        assert_eq!(drive_letters(0), Vec::<char>::new());
        assert_eq!(drive_letters(0b1100), vec!['C', 'D']);
        assert_eq!(drive_letters(1 | 1 << 25), vec!['A', 'Z']);
    }

    #[test]
    fn drive_display_name_uses_the_label_or_local_disk() {
        assert_eq!(drive_display_name("", 'C'), "Local Disk (C:)");
        assert_eq!(drive_display_name("  ", 'D'), "Local Disk (D:)");
        assert_eq!(drive_display_name("Backup", 'E'), "Backup (E:)");
    }

    #[test]
    fn wsl_distros_skip_docker_and_map_to_unc_paths() {
        assert!(is_user_wsl_distro("Ubuntu-24.04"));
        assert!(!is_user_wsl_distro("docker-desktop"));
        assert!(!is_user_wsl_distro("docker-desktop-data"));
        assert!(!is_user_wsl_distro("  "));
        assert_eq!(wsl_path("Ubuntu"), r"\\wsl$\Ubuntu\");
    }
}
