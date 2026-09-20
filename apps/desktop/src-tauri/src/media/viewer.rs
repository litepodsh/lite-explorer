//! A single reusable window that shows one media file — image, video, audio or PDF —
//! with no explorer chrome, and can toggle fullscreen. Mirrors the Settings window
//! pattern so it never re-opens if it is already on screen.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

pub const VIEWER_WINDOW: &str = "viewer";
pub const VIEWER_OPEN: &str = "viewer-open";

/// The path the viewer should show. Read on mount, and pushed on `viewer-open`
/// when the window is reused for another file.
#[derive(Default)]
pub struct ViewerTarget(pub Mutex<Option<String>>);

#[derive(Clone, Serialize)]
struct ViewerPayload {
    path: String,
}

/// Opens the viewer for `path`, reusing and focusing the existing window if there is one.
#[tauri::command]
pub async fn open_viewer(app: AppHandle, path: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(VIEWER_WINDOW) {
        let _ = app.emit_to(VIEWER_WINDOW, VIEWER_OPEN, ViewerPayload { path });
        window.show().map_err(|error| error.to_string())?;
        return window.set_focus().map_err(|error| error.to_string());
    }
    if let Some(state) = app.try_state::<ViewerTarget>() {
        *state.0.lock().unwrap() = Some(path);
    }
    let builder = WebviewWindowBuilder::new(&app, VIEWER_WINDOW, WebviewUrl::App("viewer".into()))
        .title("Preview")
        .inner_size(1100.0, 780.0)
        .min_inner_size(480.0, 360.0)
        .resizable(true)
        .focused(true)
        .background_color(crate::app::window::black())
        .center();
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(20.0, 27.0));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build().map_err(|error| error.to_string())?;
    crate::app::window::paint_black(&window);
    // The window takes focus so keyboard shortcuts (Esc, Cmd/Ctrl+W) work right away.
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

/// The path the viewer window should display on first mount.
#[tauri::command]
pub fn viewer_target(state: State<'_, ViewerTarget>) -> Option<String> {
    state.0.lock().unwrap().clone()
}
