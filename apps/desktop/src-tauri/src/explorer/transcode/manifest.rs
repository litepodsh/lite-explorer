//! The ffmpeg builds the app can download, pinned by URL, size and SHA-256.
//!
//! Every entry is a versioned URL whose content never changes, so the hash stays valid.
//! To move to a new ffmpeg, change `VERSION` and the entries together.

use std::env::consts::{ARCH, OS};

/// The ffmpeg version every entry below provides. It names the folder the app installs into.
pub const VERSION: &str = "9.0.2";

/// A file inside a downloaded zip, and the name it gets on disk.
pub struct Entry {
    pub inside: &'static str,
    pub name: &'static str,
}

pub struct Item {
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub entries: &'static [Entry],
}

const FFMPEG: Entry = Entry { inside: "ffmpeg", name: "ffmpeg" };
const FFPROBE: Entry = Entry { inside: "ffprobe", name: "ffprobe" };

const MAC_ARM64: [Item; 2] = [
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip",
        sha256: "c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924",
        size: 28_395_699,
        entries: &[FFMPEG],
    },
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffprobe.zip",
        sha256: "fcbe839537485eaee7a7a8bc5cbc0f90d53617e80943e8a5b2e31cb851197ea6",
        size: 28_317_701,
        entries: &[FFPROBE],
    },
];

const MAC_X64: [Item; 2] = [
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/macos/amd64/1789931006_9.0.2/ffmpeg.zip",
        sha256: "7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4",
        size: 33_816_391,
        entries: &[FFMPEG],
    },
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/macos/amd64/1789931006_9.0.2/ffprobe.zip",
        sha256: "2322438ed2f6319a691291b247d09c69dcaa3a982460d1f269a7e1af335cfdfd",
        size: 33_719_233,
        entries: &[FFPROBE],
    },
];

const LINUX_X64: [Item; 2] = [
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/linux/amd64/1789931100_9.0.2/ffmpeg.zip",
        sha256: "fa8ecf4abbd290d98f7d188b8649cc6b391ae209a98452be955a15aab1909d7f",
        size: 33_326_445,
        entries: &[FFMPEG],
    },
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/linux/amd64/1789931100_9.0.2/ffprobe.zip",
        sha256: "3f428c49070be3d24ec338602b76d412e401ffcb8a5641ef0e729181a232fc32",
        size: 33_223_234,
        entries: &[FFPROBE],
    },
];

const LINUX_ARM64: [Item; 2] = [
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/linux/arm64/1789931697_9.0.2/ffmpeg.zip",
        sha256: "93a76ae90db5474eecdf951a729857c64f3de23567228d6a7d5e6e8e3cd1021b",
        size: 28_753_826,
        entries: &[FFMPEG],
    },
    Item {
        url: "https://ffmpeg.martin-riedl.de/download/linux/arm64/1789931697_9.0.2/ffprobe.zip",
        sha256: "bcbe80fb741c180083327afaf5434812e006b33cacde2016b9aeaf6936128330",
        size: 28_655_683,
        entries: &[FFPROBE],
    },
];

/// One archive holds both tools. Windows on ARM runs this x64 build through emulation.
const WINDOWS_X64: [Item; 1] = [Item {
    url: "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip",
    sha256: "60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba",
    size: 114_768_076,
    entries: &[
        Entry {
            inside: "ffmpeg-9.0.2-essentials_build/bin/ffmpeg.exe",
            name: "ffmpeg.exe",
        },
        Entry {
            inside: "ffmpeg-9.0.2-essentials_build/bin/ffprobe.exe",
            name: "ffprobe.exe",
        },
    ],
}];

/// The downloads for a system, or `None` where there is no build to offer.
pub fn items_for(os: &str, arch: &str) -> Option<&'static [Item]> {
    match (os, arch) {
        ("macos", "aarch64") => Some(&MAC_ARM64),
        ("macos", "x86_64") => Some(&MAC_X64),
        ("linux", "x86_64") => Some(&LINUX_X64),
        ("linux", "aarch64") => Some(&LINUX_ARM64),
        ("windows", "x86_64" | "aarch64") => Some(&WINDOWS_X64),
        _ => None,
    }
}

/// The downloads for the system the app runs on.
pub fn items() -> Option<&'static [Item]> {
    items_for(OS, ARCH)
}

pub fn total_size(items: &[Item]) -> u64 {
    items.iter().map(|item| item.size).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEMS: [(&str, &str); 6] = [
        ("macos", "aarch64"),
        ("macos", "x86_64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
        ("windows", "x86_64"),
        ("windows", "aarch64"),
    ];

    #[test]
    fn every_supported_system_has_downloads() {
        for (os, arch) in SYSTEMS {
            assert!(items_for(os, arch).is_some(), "{os} {arch}");
        }
        assert!(items_for("freebsd", "x86_64").is_none());
        assert!(items_for("linux", "riscv64").is_none());
    }

    #[test]
    fn entries_are_pinned_and_well_formed() {
        for (os, arch) in SYSTEMS {
            for item in items_for(os, arch).unwrap() {
                assert!(item.url.starts_with("https://"), "{}", item.url);
                assert!(item.url.contains(VERSION), "{} is not version {VERSION}", item.url);
                assert_eq!(item.sha256.len(), 64, "{}", item.url);
                assert!(
                    item.sha256.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
                    "{}",
                    item.url
                );
                assert!(item.size > 1_000_000, "{}", item.url);
                assert!(item.url.ends_with(".zip"), "{}", item.url);
            }
        }
    }

    #[test]
    fn each_system_installs_both_tools() {
        for (os, arch) in SYSTEMS {
            let mut names: Vec<&str> = items_for(os, arch)
                .unwrap()
                .iter()
                .flat_map(|item| item.entries.iter().map(|entry| entry.name))
                .collect();
            names.sort_unstable();
            let exe = if os == "windows" { ".exe" } else { "" };
            assert_eq!(names, [format!("ffmpeg{exe}"), format!("ffprobe{exe}")], "{os} {arch}");
        }
    }

    #[test]
    fn total_size_adds_up() {
        let items = items_for("macos", "aarch64").unwrap();
        assert_eq!(total_size(items), 28_395_699 + 28_317_701);
    }
}
