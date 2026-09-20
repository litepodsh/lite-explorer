use std::fs;

use serde::{Deserialize, Serialize};
use tauri::{Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

pub const WINDOW_STATE_FILE: &str = "window-state.json";

#[derive(Deserialize, Serialize)]
pub struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

pub fn save_window_state(window: &WebviewWindow) {
    let (Ok(position), Ok(size), Ok(path)) = (
        window.outer_position(),
        window.outer_size(),
        window.app_handle().path().app_config_dir(),
    ) else {
        return;
    };
    let _ = fs::create_dir_all(&path);
    let _ = fs::write(
        path.join(WINDOW_STATE_FILE),
        serde_json::to_vec(&WindowState {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
        .unwrap_or_default(),
    );
}

pub fn restore_window_state(window: &WebviewWindow) {
    let Ok(path) = window.app_handle().path().app_config_dir() else {
        return;
    };
    let Some(state) = fs::read(path.join(WINDOW_STATE_FILE))
        .ok()
        .and_then(|saved| serde_json::from_slice::<WindowState>(&saved).ok())
    else {
        return;
    };
    // A degenerate size (saved while the window never got a usable size) would reopen the app as a
    // tiny 2px window. Fall back to the defaults in tauri.conf.json instead. The floor matches the
    // window's configured minWidth/minHeight.
    const MIN_WIDTH: u32 = 768;
    const MIN_HEIGHT: u32 = 500;
    if state.width < MIN_WIDTH || state.height < MIN_HEIGHT {
        return;
    }
    let monitor = window
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|monitor| {
                let area = monitor.work_area();
                state.x < area.position.x + area.size.width as i32
                    && state.x + state.width as i32 > area.position.x
                    && state.y < area.position.y + area.size.height as i32
                    && state.y + state.height as i32 > area.position.y
            })
        })
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let area = monitor.work_area();
    let (position, size) = clamp_window_state(&state, area.position, area.size);
    let _ = window.set_size(size);
    let _ = window.set_position(position);
}

pub fn clamp_window_state(
    state: &WindowState,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let width = state.width.min(size.width);
    let height = state.height.min(size.height);
    let x = state.x.clamp(
        position.x,
        position.x + size.width.saturating_sub(width) as i32,
    );
    let y = state.y.clamp(
        position.y,
        position.y + size.height.saturating_sub(height) as i32,
    );
    (
        PhysicalPosition::new(x, y),
        PhysicalSize::new(width, height),
    )
}

/// Opaque black, used for every window so no white frame shows while a page loads.
pub fn black() -> tauri::utils::config::Color {
    tauri::utils::config::Color(0, 0, 0, 255)
}

/// Forces the window and its webview to an opaque black background.
///
/// On Windows and Linux this covers both layers. On macOS `set_background_color` only
/// reaches the window layer (see its platform notes), so the webview's under-page color
/// is set separately in `set_webview_background`.
pub fn paint_black(window: &tauri::WebviewWindow) {
    let _ = window.set_background_color(Some(black()));
    #[cfg(target_os = "macos")]
    set_webview_background(window);
}

/// Paints the WKWebView's under-page color black. It is white by default and shows through
/// while the page is loading (about:blank) and when overscrolling, which causes a white
/// flash on startup that the window `backgroundColor` does not cover.
#[cfg(target_os = "macos")]
pub fn set_webview_background(window: &tauri::WebviewWindow) {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject, Bool},
        sel,
    };

    let _ = window.with_webview(|webview| unsafe {
        let Some(color_class) = AnyClass::get(c"NSColor") else {
            return;
        };
        let black: *mut AnyObject = msg_send![color_class, blackColor];
        if black.is_null() {
            return;
        }
        let wk_webview = webview.inner() as *mut AnyObject;
        let responds: Bool =
            msg_send![wk_webview, respondsToSelector: sel!(setUnderPageBackgroundColor:)];
        if responds.as_bool() {
            let _: () = msg_send![wk_webview, setUnderPageBackgroundColor: black];
        }
    });
}

#[cfg(target_os = "macos")]
pub fn unlock_webview_frame_rate(window: &tauri::WebviewWindow) {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject, Bool},
        sel,
    };
    use std::ffi::{c_char, CStr};

    let _ = window.with_webview(|webview| unsafe {
        let Some(preferences_class) = AnyClass::get(c"WKPreferences") else {
            return;
        };
        let responds: Bool = msg_send![preferences_class, respondsToSelector: sel!(_features)];
        if !responds.as_bool() {
            return;
        }

        let wk_webview = webview.inner() as *mut AnyObject;
        let configuration: *mut AnyObject = msg_send![wk_webview, configuration];
        let preferences: *mut AnyObject = msg_send![configuration, preferences];
        let can_set: Bool =
            msg_send![preferences, respondsToSelector: sel!(_setEnabled:forFeature:)];
        if preferences.is_null() || !can_set.as_bool() {
            return;
        }

        let features: *mut AnyObject = msg_send![preferences_class, _features];
        let count: usize = msg_send![features, count];
        for index in 0..count {
            let feature: *mut AnyObject = msg_send![features, objectAtIndex: index];
            let key: *mut AnyObject = msg_send![feature, key];
            let utf8: *const c_char = msg_send![key, UTF8String];
            if !utf8.is_null()
                && CStr::from_ptr(utf8).to_bytes() == b"PreferPageRenderingUpdatesNear60FPSEnabled"
            {
                let _: () = msg_send![preferences, _setEnabled: Bool::NO, forFeature: feature];
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_state_is_clamped_to_the_visible_work_area() {
        let (position, size) = clamp_window_state(
            &WindowState {
                x: 3_000,
                y: -200,
                width: 2_000,
                height: 1_000,
            },
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1_440, 900),
        );
        assert_eq!(position, PhysicalPosition::new(0, 0));
        assert_eq!(size, PhysicalSize::new(1_440, 900));
    }
}
