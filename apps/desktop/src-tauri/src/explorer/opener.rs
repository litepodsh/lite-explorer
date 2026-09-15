use std::{
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;

use crate::app::icons;

#[derive(Serialize)]
pub struct AppInfo {
    name: String,
    path: String,
    bundle_id: String,
    icon: Option<String>,
}

#[tauri::command]
pub fn open_with_apps(path: String) -> Result<Vec<AppInfo>, String> {
    #[cfg(target_os = "macos")]
    {
        use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
        use core_foundation_sys::base::{CFRelease, CFTypeRef};
        use core_foundation_sys::string::{
            kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString,
            CFStringGetLength, CFStringRef,
        };
        use core_foundation_sys::url::{kCFURLPOSIXPathStyle, CFURLCopyFileSystemPath, CFURLRef};
        use std::ffi::{CStr, CString};
        use std::path::Path;
        use std::ptr;

        #[link(name = "CoreServices", kind = "framework")]
        extern "C" {
            fn LSCopyApplicationURLsForURL(inURL: CFURLRef, roleMask: u32) -> CFArrayRef;
            fn LSCopyDisplayNameForURL(inURL: CFURLRef, outDisplayName: *mut CFStringRef) -> u8;
        }

        const K_LS_ROLES_ALL: u32 = 0xFFFF_FFFF;

        unsafe fn cfurl_from_path(path: &str) -> Option<CFURLRef> {
            let cstr = CString::new(path).ok()?;
            let cfstr: CFStringRef =
                CFStringCreateWithCString(ptr::null(), cstr.as_ptr(), kCFStringEncodingUTF8);
            if cfstr.is_null() {
                return None;
            }
            let url = core_foundation_sys::url::CFURLCreateWithFileSystemPath(
                ptr::null(),
                cfstr,
                kCFURLPOSIXPathStyle,
                0,
            );
            CFRelease(cfstr as CFTypeRef);
            if url.is_null() {
                None
            } else {
                Some(url)
            }
        }

        unsafe fn cfstring_to_string(string: CFStringRef) -> String {
            if string.is_null() {
                return String::new();
            }
            let length = CFStringGetLength(string);
            let mut buffer = vec![0i8; (length as usize) * 4 + 1];
            let ok = CFStringGetCString(
                string,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                kCFStringEncodingUTF8,
            );
            if ok == 0 {
                return String::new();
            }
            CStr::from_ptr(buffer.as_ptr())
                .to_string_lossy()
                .into_owned()
        }

        unsafe {
            let url = match cfurl_from_path(&path) {
                Some(url) => url,
                None => return Ok(vec![]),
            };
            let apps = LSCopyApplicationURLsForURL(url, K_LS_ROLES_ALL);
            CFRelease(url as CFTypeRef);
            if apps.is_null() {
                return Ok(vec![]);
            }
            let count = CFArrayGetCount(apps);
            let mut result = Vec::with_capacity(count as usize);
            for index in 0..count {
                let app_url = CFArrayGetValueAtIndex(apps, index) as CFURLRef;
                if app_url.is_null() {
                    continue;
                }
                let fs_path = CFURLCopyFileSystemPath(app_url, kCFURLPOSIXPathStyle);
                let app_path = cfstring_to_string(fs_path);
                CFRelease(fs_path as CFTypeRef);
                let mut display: CFStringRef = ptr::null_mut();
                LSCopyDisplayNameForURL(app_url, &mut display);
                let name = if display.is_null() {
                    Path::new(&app_path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    let string = cfstring_to_string(display);
                    CFRelease(display as CFTypeRef);
                    string
                };
                result.push(AppInfo {
                    name,
                    path: app_path,
                    bundle_id: String::new(),
                    icon: None,
                });
            }
            CFRelease(apps as CFTypeRef);
            // De-duplicate by path (LaunchServices can return duplicates).
            let mut seen = std::collections::HashSet::new();
            result.retain(|app| seen.insert(app.path.clone()));
            for app in &mut result {
                app.icon = app_icon_data_url(&app.path);
            }
            Ok(result)
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Ok(vec![])
    }
}

/// The application the OS uses to open `path` by default (what a double-click in Finder launches).
#[tauri::command]
pub fn default_app(path: String) -> Option<AppInfo> {
    #[cfg(target_os = "macos")]
    {
        use objc2::{class, msg_send, rc::autoreleasepool, runtime::AnyObject};
        use std::ffi::{c_char, CStr, CString};

        let c_path = CString::new(path).ok()?;
        let app_path = autoreleasepool(|_| unsafe {
            let string: *mut AnyObject =
                msg_send![class!(NSString), stringWithUTF8String: c_path.as_ptr()];
            let url: *mut AnyObject = msg_send![class!(NSURL), fileURLWithPath: string];
            let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
            let app_url: *mut AnyObject = msg_send![workspace, URLForApplicationToOpenURL: url];
            if app_url.is_null() {
                return None;
            }
            let app_path: *mut AnyObject = msg_send![app_url, path];
            let utf8: *const c_char = msg_send![app_path, UTF8String];
            (!utf8.is_null()).then(|| CStr::from_ptr(utf8).to_string_lossy().into_owned())
        })?;
        let name = Path::new(&app_path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| app_path.clone());
        let icon = app_icon_data_url(&app_path);
        Some(AppInfo {
            name,
            path: app_path,
            bundle_id: String::new(),
            icon,
        })
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// PNG data URL of the OS icon for `path`, cached per path.
pub fn app_icon_data_url(path: &str) -> Option<String> {
    icons::icon_data_url(path)
}

#[tauri::command]
pub fn open_with(path: String, app_path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = Command::new("open")
            .args(["-a", &app_path, &path])
            .status()
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("failed to open with {app_path} (exit {status})"))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, app_path);
        Err("Open With is only supported on macOS".into())
    }
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");

    let status = command
        .arg(&path)
        .status()
        .map_err(|error| error.to_string())?;
    // explorer.exe returns exit code 1 even when it succeeds.
    if status.success() || cfg!(target_os = "windows") {
        Ok(())
    } else {
        Err(format!("failed to open {path} (exit {status})"))
    }
}

/// A terminal the app can launch, reported to the settings UI.
#[derive(Serialize, Clone)]
pub struct TerminalInfo {
    id: String,
    name: String,
    /// PNG data URL of the app icon, when the terminal ships a bundle.
    icon: Option<String>,
}

pub struct TerminalSpec {
    id: &'static str,
    name: &'static str,
    /// Bundle paths relative to an application folder (macOS).
    bundles: &'static [&'static str],
    /// Executable names looked up on `PATH`.
    binaries: &'static [&'static str],
}

/// Terminals the app knows how to launch. Detection keeps only the installed ones.
pub const KNOWN_TERMINALS: &[TerminalSpec] = &[
    TerminalSpec {
        id: "terminal",
        name: "Terminal",
        bundles: &["Utilities/Terminal.app", "Terminal.app"],
        binaries: &[],
    },
    TerminalSpec {
        id: "iterm",
        name: "iTerm2",
        bundles: &["iTerm.app"],
        binaries: &[],
    },
    TerminalSpec {
        id: "ghostty",
        name: "Ghostty",
        bundles: &["Ghostty.app"],
        binaries: &["ghostty"],
    },
    TerminalSpec {
        id: "wezterm",
        name: "WezTerm",
        bundles: &["WezTerm.app"],
        binaries: &["wezterm"],
    },
    TerminalSpec {
        id: "warp",
        name: "Warp",
        bundles: &["Warp.app"],
        binaries: &[],
    },
    TerminalSpec {
        id: "alacritty",
        name: "Alacritty",
        bundles: &["Alacritty.app"],
        binaries: &["alacritty"],
    },
    TerminalSpec {
        id: "kitty",
        name: "kitty",
        bundles: &["kitty.app"],
        binaries: &["kitty"],
    },
    TerminalSpec {
        id: "hyper",
        name: "Hyper",
        bundles: &["Hyper.app"],
        binaries: &[],
    },
    TerminalSpec {
        id: "tabby",
        name: "Tabby",
        bundles: &["Tabby.app"],
        binaries: &[],
    },
    TerminalSpec {
        id: "rio",
        name: "Rio",
        bundles: &["Rio.app"],
        binaries: &["rio"],
    },
    TerminalSpec {
        id: "wt",
        name: "Windows Terminal",
        bundles: &[],
        binaries: &["wt"],
    },
    TerminalSpec {
        id: "cmd",
        name: "Command Prompt",
        bundles: &[],
        binaries: &["cmd"],
    },
    TerminalSpec {
        id: "powershell",
        name: "PowerShell",
        bundles: &[],
        binaries: &["pwsh", "powershell"],
    },
    TerminalSpec {
        id: "gnome-terminal",
        name: "GNOME Terminal",
        bundles: &[],
        binaries: &["gnome-terminal"],
    },
    TerminalSpec {
        id: "konsole",
        name: "Konsole",
        bundles: &[],
        binaries: &["konsole"],
    },
    TerminalSpec {
        id: "xfce4-terminal",
        name: "Xfce Terminal",
        bundles: &[],
        binaries: &["xfce4-terminal"],
    },
    TerminalSpec {
        id: "tilix",
        name: "Tilix",
        bundles: &[],
        binaries: &["tilix"],
    },
    TerminalSpec {
        id: "terminator",
        name: "Terminator",
        bundles: &[],
        binaries: &["terminator"],
    },
    TerminalSpec {
        id: "foot",
        name: "foot",
        bundles: &[],
        binaries: &["foot"],
    },
    TerminalSpec {
        id: "xterm",
        name: "xterm",
        bundles: &[],
        binaries: &["xterm"],
    },
];

/// Application folders searched for terminal bundles (macOS).
pub fn app_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots
}

pub fn binary_on_path(name: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    let names: Vec<String> = if cfg!(target_os = "windows") {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    };
    std::env::split_paths(&paths)
        .any(|dir| names.iter().any(|candidate| dir.join(candidate).is_file()))
}

/// Terminals installed on this machine, in the order of [`KNOWN_TERMINALS`].
#[tauri::command]
pub fn detect_terminals() -> Vec<TerminalInfo> {
    let roots = app_roots();
    KNOWN_TERMINALS
        .iter()
        .filter_map(|spec| {
            let bundle = spec.bundles.iter().find_map(|bundle| {
                roots
                    .iter()
                    .map(|root| root.join(bundle))
                    .find(|candidate| candidate.exists())
            });
            let installed =
                bundle.is_some() || spec.binaries.iter().any(|name| binary_on_path(name));
            if !installed {
                return None;
            }
            Some(TerminalInfo {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                icon: bundle.and_then(|path| app_icon_data_url(&path.to_string_lossy())),
            })
        })
        .collect()
}

pub fn shell_quote(path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!("\"{}\"", path.replace('"', "\"\""))
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

pub fn run_custom_terminal(template: &str, path: &str) -> Result<(), String> {
    let command = template.replace("{path}", &shell_quote(path));
    #[cfg(target_os = "windows")]
    let result = Command::new("cmd").args(["/C", &command]).spawn();
    #[cfg(not(target_os = "windows"))]
    let result = Command::new("/bin/sh").arg("-c").arg(&command).spawn();
    result.map(|_| ()).map_err(|error| error.to_string())
}

/// Opens a macOS app terminal, which cds to the folder it is given.
#[cfg(target_os = "macos")]
pub fn open_app_terminal(app: &str, path: &str) -> Result<(), String> {
    let status = Command::new("open")
        .args(["-a", app, path])
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open {app} in {path} (exit {status})"))
    }
}

/// The command line that opens `terminal` in `path`, or `None` when unknown.
pub fn invocation_for(terminal: &str, path: &str) -> Option<(&'static str, Vec<String>)> {
    let invocation = match terminal {
        "wt" => ("wt", vec!["-d".to_string(), path.to_string()]),
        "cmd" => (
            "cmd",
            vec![
                "/C".into(),
                "start".into(),
                "cmd".into(),
                "/K".into(),
                format!("cd /d {path}"),
            ],
        ),
        "powershell" => (
            "powershell",
            vec![
                "-NoExit".into(),
                "-Command".into(),
                format!("Set-Location -LiteralPath '{}'", path.replace('\'', "''")),
            ],
        ),
        "gnome-terminal" => (
            "gnome-terminal",
            vec![format!("--working-directory={path}")],
        ),
        "konsole" => ("konsole", vec!["--workdir".into(), path.into()]),
        "xfce4-terminal" => (
            "xfce4-terminal",
            vec![format!("--working-directory={path}")],
        ),
        "tilix" => ("tilix", vec![format!("--working-directory={path}")]),
        "terminator" => (
            "terminator",
            vec!["--working-directory".into(), path.into()],
        ),
        "foot" => ("foot", vec![format!("--working-directory={path}")]),
        "xterm" => (
            "xterm",
            vec![
                "-e".into(),
                "sh".into(),
                "-c".into(),
                format!("cd {}; exec $SHELL", shell_quote(path)),
            ],
        ),
        "ghostty" => ("ghostty", vec![format!("--working-directory={path}")]),
        "alacritty" => ("alacritty", vec!["--working-directory".into(), path.into()]),
        "kitty" => ("kitty", vec!["--directory".into(), path.into()]),
        "wezterm" => ("wezterm", vec!["start".into(), "--cwd".into(), path.into()]),
        "rio" => ("rio", vec!["--working-dir".into(), path.into()]),
        _ => return None,
    };
    Some(invocation)
}

/// The macOS app that ships `terminal`, for the bundle fallback.
#[cfg(target_os = "macos")]
pub fn macos_app_for(terminal: &str) -> Option<&'static str> {
    match terminal {
        "ghostty" => Some("Ghostty"),
        "wezterm" => Some("WezTerm"),
        "alacritty" => Some("Alacritty"),
        "kitty" => Some("kitty"),
        "rio" => Some("Rio"),
        _ => None,
    }
}

/// Runs a terminal binary from `PATH`, falling back to an app bundle on macOS.
pub fn spawn_program(program: &str, args: &[String], terminal: &str) -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    let _ = terminal;
    match Command::new(program).args(args).spawn() {
        Ok(_) => Ok(()),
        Err(error) => {
            #[cfg(target_os = "macos")]
            if let Some(app) = macos_app_for(terminal) {
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                return open_app_terminal_with_args(app, &refs);
            }
            Err(error.to_string())
        }
    }
}

#[cfg(target_os = "macos")]
pub fn open_app_terminal_with_args(app: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new("open")
        .arg("-a")
        .arg(app)
        .arg("--args")
        .args(args)
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open {app} (exit {status})"))
    }
}

/// Opens the operating system's default terminal in `path`.
pub fn open_system_terminal(directory: &Path, path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = directory;
        open_app_terminal("Terminal", path)
    }

    #[cfg(target_os = "windows")]
    {
        let _ = directory;
        // Windows Terminal when installed, otherwise the classic console.
        if Command::new("wt").args(["-d", path]).spawn().is_ok() {
            return Ok(());
        }
        Command::new("cmd")
            .args(["/C", "start", "cmd", "/K", &format!("cd /d {path}")])
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let candidates: Vec<String> = std::env::var("TERMINAL")
            .ok()
            .into_iter()
            .chain(
                [
                    "x-terminal-emulator",
                    "gnome-terminal",
                    "konsole",
                    "xfce4-terminal",
                    "xterm",
                ]
                .into_iter()
                .map(String::from),
            )
            .collect();
        let mut last_error = None;
        for candidate in candidates {
            match Command::new(&candidate).current_dir(directory).spawn() {
                Ok(_) => return Ok(()),
                Err(error) => last_error = Some(error),
            }
        }
        match last_error {
            Some(error) => Err(format!("failed to open a terminal in {path}: {error}")),
            None => Err(format!("failed to open a terminal in {path}")),
        }
    }
}

/// Opens `terminal` in `path`, where `terminal` is an id from [`detect_terminals`],
/// "system" for the OS default, or "custom" with a `{path}` command template.
#[tauri::command]
pub fn open_terminal(
    path: String,
    terminal: String,
    custom_command: Option<String>,
) -> Result<(), String> {
    let directory = Path::new(&path);
    if !directory.is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    if terminal == "custom" {
        let template = custom_command.unwrap_or_default();
        if template.trim().is_empty() {
            return Err("Custom terminal command is empty".into());
        }
        return run_custom_terminal(&template, &path);
    }
    match terminal.as_str() {
        "system" | "" | "terminal" => open_system_terminal(directory, &path),
        #[cfg(target_os = "macos")]
        "ghostty" => open_app_terminal("Ghostty", &path),
        #[cfg(target_os = "macos")]
        "iterm" => open_app_terminal("iTerm", &path),
        #[cfg(target_os = "macos")]
        "hyper" => open_app_terminal("Hyper", &path),
        #[cfg(target_os = "macos")]
        "tabby" => open_app_terminal("Tabby", &path),
        #[cfg(target_os = "macos")]
        "warp" => {
            let encoded: String =
                percent_encoding::utf8_percent_encode(&path, percent_encoding::NON_ALPHANUMERIC)
                    .to_string();
            open_app_terminal_with_uri(&format!("warp://action/new_tab?path={encoded}"))
        }
        other => match invocation_for(other, &path) {
            Some((program, args)) => spawn_program(program, &args, other),
            None => open_system_terminal(directory, &path),
        },
    }
}

#[cfg(target_os = "macos")]
pub fn open_app_terminal_with_uri(uri: &str) -> Result<(), String> {
    let status = Command::new("open")
        .arg(uri)
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open {uri} (exit {status})"))
    }
}

/// Reveals `path` in the system file manager, selecting it when the OS supports it.
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = Command::new("open").arg("-R").arg(&path).status();
    #[cfg(target_os = "windows")]
    let status = Command::new("explorer")
        .arg(format!("/select,{path}"))
        .status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = {
        let parent = Path::new(&path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        Command::new("xdg-open").arg(parent).status()
    };

    status
        .map_err(|error| error.to_string())
        .and_then(|status| {
            // explorer.exe returns exit code 1 even when it succeeds.
            if status.success() || cfg!(target_os = "windows") {
                Ok(())
            } else {
                Err(format!("failed to reveal {path} (exit {status})"))
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn app_icon_is_a_png_data_url() {
        let icon = app_icon_data_url("/System/Applications/Calculator.app").unwrap();
        assert!(icon.starts_with("data:image/png;base64,"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn default_app_resolves_for_a_text_file() {
        let path = std::env::temp_dir().join("liteexplorer-default-app.txt");
        std::fs::write(&path, "hello").unwrap();
        let app = default_app(path.to_string_lossy().into_owned()).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(app.path.ends_with(".app"));
        assert!(!app.name.is_empty() && !app.name.ends_with(".app"));
    }
}
