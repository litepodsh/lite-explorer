//! Finds ffmpeg and ffprobe, and reads what the build can do.

use std::{
    collections::BTreeSet,
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde::Serialize;

use super::options::HardwareEncoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Downloaded by the app into its own folder.
    Managed,
    /// Already installed on the system.
    System,
}

#[derive(Clone, Debug)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub source: Source,
    pub version: String,
}

/// Encoders the converter can use. Anything else in the build is ignored.
pub const WANTED_ENCODERS: [&str; 10] = [
    "libx264",
    "libvpx-vp9",
    "aac",
    "libopus",
    "libmp3lame",
    "flac",
    "pcm_s16le",
    "gif",
    "libwebp",
    "libwebp_anim",
];

pub fn executable(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Folders searched after the app's own: `PATH` plus the places package managers install to,
/// which a GUI app launched from Finder or the Start menu does not always have on its `PATH`.
pub fn system_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let path = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in path {
        if !dir.as_os_str().is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    let extras: Vec<PathBuf> = if cfg!(windows) {
        let mut list = vec![PathBuf::from(r"C:\ProgramData\chocolatey\bin")];
        if let Some(local) = env::var_os("LOCALAPPDATA") {
            list.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
        }
        if let Some(profile) = env::var_os("USERPROFILE") {
            list.push(PathBuf::from(profile).join(r"scoop\shims"));
        }
        list
    } else {
        ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/snap/bin"]
            .into_iter()
            .map(PathBuf::from)
            .collect()
    };
    for dir in extras {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// "ffmpeg version 9.0.2-https://example Copyright..." gives "9.0.2-https://example".
pub fn parse_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    let rest = line.split("version ").nth(1)?;
    let version = rest.split_whitespace().next()?;
    (!version.is_empty()).then(|| version.to_string())
}

/// Encoder names from `ffmpeg -encoders`. Lines look like ` V....D libx264   libx264 H.264`.
pub fn parse_encoders(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("------"))
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            let is_encoder_line = flags.len() == 6 && matches!(flags.chars().next(), Some('V' | 'A' | 'S'));
            is_encoder_line.then(|| name.to_string())
        })
        .collect()
}

fn output_of(program: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Both tools in `dir`, if ffmpeg runs.
pub fn tools_in(dir: &Path, source: Source) -> Option<Tools> {
    let ffmpeg = dir.join(executable("ffmpeg"));
    let ffprobe = dir.join(executable("ffprobe"));
    if !ffmpeg.is_file() || !ffprobe.is_file() {
        return None;
    }
    let version = parse_version(&output_of(&ffmpeg, &["-version"])?)?;
    Some(Tools {
        ffmpeg,
        ffprobe,
        source,
        version,
    })
}

/// The app's own copy first, then the first system folder that has a working pair.
pub fn locate(managed_dir: Option<&Path>, search: &[PathBuf]) -> Option<Tools> {
    managed_dir
        .and_then(|dir| tools_in(dir, Source::Managed))
        .or_else(|| search.iter().find_map(|dir| tools_in(dir, Source::System)))
}

/// The graphics hardware encoder that really works on this machine, if any. Listing an encoder
/// proves nothing (a build lists NVENC on a PC without an Nvidia card), so a tiny clip is encoded
/// with the same arguments a real job would use.
pub fn detect_hardware(tools: &Tools) -> Option<HardwareEncoder> {
    let candidates: &[HardwareEncoder] = if cfg!(target_os = "macos") {
        &[HardwareEncoder::VideoToolbox]
    } else {
        &[HardwareEncoder::Nvenc]
    };
    candidates.iter().copied().find(|encoder| {
        Command::new(&tools.ffmpeg)
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", "color=c=black:s=256x256:d=0.2"])
            .args(encoder.video_args(50))
            .args(["-pix_fmt", "yuv420p", "-f", "null", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Which of `WANTED_ENCODERS` this ffmpeg build has.
pub fn encoders(tools: &Tools) -> Vec<String> {
    let available = output_of(&tools.ffmpeg, &["-hide_banner", "-encoders"])
        .map(|text| parse_encoders(&text))
        .unwrap_or_default();
    WANTED_ENCODERS
        .iter()
        .filter(|name| available.contains(**name))
        .map(|name| name.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const ENCODERS: &str = "Encoders:
 V..... = Video
 A..... = Audio
 S..... = Subtitle
 .F.... = Frame-level multithreading
 ------
 V....D libx264              libx264 H.264 / AVC / MPEG-4 AVC / MPEG-4 part 10 (codec h264)
 V....D libvpx-vp9           libvpx VP9 (codec vp9)
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libmp3lame           libmp3lame MP3 (MPEG audio layer 3) (codec mp3)
 S..... srt                  SubRip subtitle
";

    #[test]
    fn reads_the_version_from_the_first_line() {
        assert_eq!(
            parse_version("ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers\nbuilt with clang"),
            Some("9.0.2".into())
        );
        assert_eq!(
            parse_version("ffmpeg version 9.0.2-https://www.martin-riedl.de Copyright (c)"),
            Some("9.0.2-https://www.martin-riedl.de".into())
        );
        assert_eq!(parse_version("ffmpeg version N-127654-g7045cc9b80 Copyright"), Some("N-127654-g7045cc9b80".into()));
        assert_eq!(parse_version("command not found"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn reads_encoder_names_after_the_legend() {
        let names = parse_encoders(ENCODERS);
        assert!(names.contains("libx264"));
        assert!(names.contains("libvpx-vp9"));
        assert!(names.contains("aac"));
        assert!(names.contains("libmp3lame"));
        assert!(names.contains("srt"));
        // The legend lines above the separator are not encoders.
        assert!(!names.contains("="));
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn an_empty_listing_has_no_encoders() {
        assert!(parse_encoders("").is_empty());
        assert!(parse_encoders("nonsense\nwithout a separator").is_empty());
    }

    #[test]
    fn system_dirs_include_the_package_manager_folders() {
        let dirs = system_dirs();
        if cfg!(windows) {
            assert!(dirs.iter().any(|dir| dir.ends_with("bin") && dir.to_string_lossy().contains("chocolatey")));
        } else {
            assert!(dirs.contains(&PathBuf::from("/opt/homebrew/bin")));
            assert!(dirs.contains(&PathBuf::from("/usr/local/bin")));
        }
        // No folder is listed twice, even when `PATH` repeats one.
        let mut unique = dirs.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), dirs.len());
        assert!(dirs.iter().all(|dir| !dir.as_os_str().is_empty()));
    }

    #[test]
    fn a_folder_without_the_tools_is_not_a_match() {
        let dir = tempfile::tempdir().unwrap();
        assert!(tools_in(dir.path(), Source::System).is_none());
        fs::write(dir.path().join(executable("ffmpeg")), "not a program").unwrap();
        assert!(tools_in(dir.path(), Source::System).is_none(), "ffprobe is missing too");
    }

    /// Against the ffmpeg installed on this machine: whatever is detected must really encode.
    #[test]
    fn detected_hardware_really_encodes() {
        let Some(tools) = locate(None, &system_dirs()) else {
            eprintln!("SKIPPED: ffmpeg is not installed");
            return;
        };
        match detect_hardware(&tools) {
            Some(encoder) => {
                let dir = tempfile::tempdir().unwrap();
                let output = dir.path().join("out.mp4");
                let status = Command::new(&tools.ffmpeg)
                    .args(["-v", "error", "-f", "lavfi", "-i", "testsrc=d=1:s=320x240:r=25"])
                    .args(encoder.video_args(60))
                    .args(["-pix_fmt", "yuv420p"])
                    .arg(&output)
                    .status()
                    .unwrap();
                assert!(status.success());
                assert!(output.metadata().unwrap().len() > 0);
            }
            None => eprintln!("no hardware encoder on this machine"),
        }
    }

    /// A fake pair of tools: shell scripts that print a version.
    #[cfg(unix)]
    fn fake_tools(dir: &Path, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        for name in ["ffmpeg", "ffprobe"] {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\necho \"{name} version {version} Copyright\"\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_apps_own_copy_wins_over_the_system() {
        let managed = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        fake_tools(managed.path(), "9.0.2");
        fake_tools(system.path(), "6.1");
        let found = locate(Some(managed.path()), &[system.path().to_path_buf()]).unwrap();
        assert_eq!(found.source, Source::Managed);
        assert_eq!(found.version, "9.0.2");
    }

    #[cfg(unix)]
    #[test]
    fn the_system_is_used_when_there_is_no_managed_copy() {
        let managed = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        fake_tools(system.path(), "6.1");
        let found = locate(
            Some(managed.path()),
            &[empty.path().to_path_buf(), system.path().to_path_buf()],
        )
        .unwrap();
        assert_eq!(found.source, Source::System);
        assert_eq!(found.version, "6.1");
        assert!(locate(None, &[empty.path().to_path_buf()]).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_install_is_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let broken = tempfile::tempdir().unwrap();
        let good = tempfile::tempdir().unwrap();
        for name in ["ffmpeg", "ffprobe"] {
            let path = broken.path().join(name);
            fs::write(&path, "#!/bin/sh\nexit 1\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fake_tools(good.path(), "7.0");
        let found = locate(None, &[broken.path().to_path_buf(), good.path().to_path_buf()]).unwrap();
        assert_eq!(found.version, "7.0");
    }

    /// Against the ffmpeg installed on this machine, when there is one.
    #[test]
    fn the_real_ffmpeg_reports_its_encoders() {
        let Some(tools) = locate(None, &system_dirs()) else {
            eprintln!("skipped: ffmpeg is not installed");
            return;
        };
        let found = encoders(&tools);
        assert!(!found.is_empty(), "{:?}", tools.ffmpeg);
        assert!(found.iter().all(|name| WANTED_ENCODERS.contains(&name.as_str())));
    }
}
