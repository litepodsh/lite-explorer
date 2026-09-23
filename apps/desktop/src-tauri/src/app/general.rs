#[tauri::command]
pub fn os_detection() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// Session hints only; no compositor process or configuration is changed.
#[tauri::command]
pub fn is_hyprland() -> bool {
    cfg!(target_os = "linux")
        && (std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
            .is_ok_and(|value| !value.trim().is_empty())
            || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|value| {
                value
                    .split(':')
                    .any(|desktop| desktop.trim().eq_ignore_ascii_case("Hyprland"))
            }))
}
