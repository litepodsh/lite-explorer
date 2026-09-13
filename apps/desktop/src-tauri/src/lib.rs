// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn os_detection() -> &'static str {
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let floating = CheckMenuItem::with_id(
                app.handle(),
                "toggle-sidebar-floating",
                "Floating Sidebar",
                true,
                false,
                Some("CmdOrCtrl+Shift+F"),
            )?;
            let view = Submenu::with_items(app.handle(), "Sidebar", true, &[&floating])?;
            let menu = Menu::default(app.handle())?;
            menu.append(&view)?;
            app.manage(SidebarMenu(floating));
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            if event.id() == "toggle-sidebar-floating" {
                let floating = app.state::<SidebarMenu>().0.is_checked().unwrap_or(false);
                let _ = app.emit("sidebar-floating", floating);
            }
        })
        .invoke_handler(tauri::generate_handler![greet, os_detection, set_sidebar_floating])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
use tauri::{
    menu::{CheckMenuItem, Menu, Submenu},
    Emitter, Manager, State,
};

struct SidebarMenu(CheckMenuItem<tauri::Wry>);

#[tauri::command]
fn set_sidebar_floating(menu: State<'_, SidebarMenu>, floating: bool) {
    let _ = menu.0.set_checked(floating);
}
