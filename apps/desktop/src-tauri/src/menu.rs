use serde::Serialize;
use tauri::{
    menu::{
        AboutMetadata, CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
        HELP_SUBMENU_ID,
    },
    AppHandle, Emitter, Manager, Runtime, State, Wry,
};

pub const CHECK_FOR_UPDATES: &str = "check-for-updates";

/// Accelerators of the app's own menu items, by id. muda does not expose an item's accelerator
/// after creation, and the title bar on Windows and Linux needs it to show and dispatch shortcuts.
const ACCELERATORS: &[(&str, &str)] = &[
    ("toggle-sidebar-floating", "CmdOrCtrl+Shift+F"),
    ("toggle-hidden-files", "CmdOrCtrl+Shift+Period"),
    ("new-folder", "CmdOrCtrl+Shift+N"),
    ("new-file", "CmdOrCtrl+Shift+Alt+N"),
    ("go-to-folder", "CmdOrCtrl+Shift+P"),
    ("new-tab", "CmdOrCtrl+T"),
    ("close-tab", "CmdOrCtrl+W"),
    ("next-tab", "CmdOrCtrl+Shift+]"),
    ("previous-tab", "CmdOrCtrl+Shift+["),
    ("new-tab-other", "CmdOrCtrl+Shift+T"),
    ("toggle-second-pane", "CmdOrCtrl+Shift+L"),
];

pub fn accelerator(id: &str) -> Option<&'static str> {
    ACCELERATORS
        .iter()
        .find(|(item, _)| *item == id)
        .map(|(_, accelerator)| *accelerator)
}

/// Removes `&` mnemonic markers; `&&` stays as a literal `&`.
pub fn strip_mnemonic(text: &str) -> String {
    text.replace("&&", "\u{0}")
        .replace('&', "")
        .replace('\u{0}', "&")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PredefinedKind {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Maximize,
    CloseWindow,
    Quit,
    Fullscreen,
    About,
}

/// Identifies a predefined item by its default label. muda does not expose the predefined type,
/// and outside macOS the labels keep their mnemonics ("&Copy", "&Exit", "&About").
pub fn predefined_kind(label: &str) -> Option<PredefinedKind> {
    let label = strip_mnemonic(label);
    Some(match label.as_str() {
        "Undo" => PredefinedKind::Undo,
        "Redo" => PredefinedKind::Redo,
        "Cut" => PredefinedKind::Cut,
        "Copy" => PredefinedKind::Copy,
        "Paste" => PredefinedKind::Paste,
        "Select All" => PredefinedKind::SelectAll,
        "Minimize" => PredefinedKind::Minimize,
        "Maximize" | "Zoom" => PredefinedKind::Maximize,
        "Close" | "Close Window" => PredefinedKind::CloseWindow,
        "Quit" | "Exit" => PredefinedKind::Quit,
        "Toggle Full Screen" => PredefinedKind::Fullscreen,
        _ if label.starts_with("About") => PredefinedKind::About,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AboutInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub copyright: String,
}

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let info = AboutInfo {
        name: "Lite Explorer".to_string(),
        version: app.package_info().version.to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
        copyright: app.config().bundle.copyright.clone().unwrap_or_default(),
    };
    let floating = CheckMenuItem::with_id(
        app,
        "toggle-sidebar-floating",
        "Floating Sidebar",
        true,
        false,
        accelerator("toggle-sidebar-floating"),
    )?;
    let sidebar = Submenu::with_items(app, "Sidebar", true, &[&floating])?;
    let hidden_files = CheckMenuItem::with_id(
        app,
        "toggle-hidden-files",
        "Show Hidden Files",
        true,
        false,
        accelerator("toggle-hidden-files"),
    )?;
    let show_fps = CheckMenuItem::with_id(
        app,
        "toggle-show-fps",
        "Show FPS",
        true,
        false,
        None::<&str>,
    )?;
    let menu = Menu::default(app)?;
    let new_folder = MenuItem::with_id(
        app,
        "new-folder",
        "New Folder",
        true,
        accelerator("new-folder"),
    )?;
    let new_file = MenuItem::with_id(app, "new-file", "New File", true, accelerator("new-file"))?;
    let open_item = MenuItem::with_id(app, "open", "Open", false, None::<&str>)?;
    let go_to_folder = MenuItem::with_id(
        app,
        "go-to-folder",
        "Go to Folder…",
        true,
        accelerator("go-to-folder"),
    )?;
    let new_tab = MenuItem::with_id(app, "new-tab", "New Tab", true, accelerator("new-tab"))?;
    let close_tab = MenuItem::with_id(
        app,
        "close-tab",
        "Close Tab",
        true,
        accelerator("close-tab"),
    )?;
    let next_tab = MenuItem::with_id(
        app,
        "next-tab",
        "Show Next Tab",
        true,
        accelerator("next-tab"),
    )?;
    let previous_tab = MenuItem::with_id(
        app,
        "previous-tab",
        "Show Previous Tab",
        true,
        accelerator("previous-tab"),
    )?;
    let new_tab_other = MenuItem::with_id(
        app,
        "new-tab-other",
        "New Tab in Second Pane",
        true,
        accelerator("new-tab-other"),
    )?;
    let show_second_pane = MenuItem::with_id(
        app,
        "toggle-second-pane",
        "Show Second Pane",
        true,
        accelerator("toggle-second-pane"),
    )?;
    let split_orientation = MenuItem::with_id(
        app,
        "toggle-pane-orientation",
        "Split Horizontally/Vertically",
        true,
        None::<&str>,
    )?;
    set_about_icon(app, &menu, &info)?;
    add_check_for_updates(app, &menu)?;
    let items = menu.items()?;
    let file_index = items.iter().position(|item| match item {
        MenuItemKind::Submenu(submenu) if submenu.text().ok().as_deref() == Some("File") => true,
        _ => false,
    });
    match file_index {
        Some(index) => {
            if let MenuItemKind::Submenu(file_submenu) = &items[index] {
                remove_close_window(file_submenu)?;
                file_submenu.insert(&new_tab, 0)?;
                file_submenu.insert(&new_tab_other, 1)?;
                file_submenu.insert(&new_folder, 2)?;
                file_submenu.insert(&new_file, 3)?;
                file_submenu.insert(&PredefinedMenuItem::separator(app)?, 4)?;
                file_submenu.insert(&open_item, 5)?;
                file_submenu.insert(&go_to_folder, 6)?;
                file_submenu.append(&PredefinedMenuItem::separator(app)?)?;
                file_submenu.append(&close_tab)?;
            }
        }
        None => {
            let file_menu = Submenu::with_items(
                app,
                "File",
                true,
                &[&new_tab, &new_tab_other, &new_folder, &new_file, &close_tab],
            )?;
            let view_index = items.iter().position(|item| match item {
                MenuItemKind::Submenu(submenu)
                    if submenu.text().ok().as_deref() == Some("View") =>
                {
                    true
                }
                _ => false,
            });
            match view_index {
                Some(index) => menu.insert(&file_menu, index)?,
                None => menu.append(&file_menu)?,
            }
        }
    }
    let view_menu = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(submenu) if submenu.text().ok().as_deref() == Some("View") => {
            Some(submenu)
        }
        _ => None,
    });
    match view_menu {
        Some(view_menu) => {
            view_menu.prepend(&hidden_files)?;
            view_menu.insert(&show_fps, 1)?;
            view_menu.insert(&sidebar, 2)?;
            view_menu.insert(&show_second_pane, 3)?;
            view_menu.insert(&split_orientation, 4)?;
            view_menu.insert(&PredefinedMenuItem::separator(app)?, 5)?;
        }
        None => menu.append(&Submenu::with_items(
            app,
            "View",
            true,
            &[
                &hidden_files,
                &show_fps,
                &sidebar,
                &show_second_pane,
                &split_orientation,
            ],
        )?)?,
    }
    let window_menu = menu.items()?.into_iter().find_map(|item| match item {
        MenuItemKind::Submenu(submenu) if submenu.text().ok().as_deref() == Some("Window") => {
            Some(submenu)
        }
        _ => None,
    });
    match window_menu {
        Some(window_menu) => {
            remove_close_window(&window_menu)?;
            window_menu.append(&PredefinedMenuItem::separator(app)?)?;
            window_menu.append(&next_tab)?;
            window_menu.append(&previous_tab)?;
        }
        None => menu.append(&Submenu::with_items(
            app,
            "Window",
            true,
            &[&next_tab, &previous_tab],
        )?)?,
    }
    app.manage(SidebarMenu(floating));
    app.manage(HiddenFilesMenu(hidden_files));
    app.manage(ShowFpsMenu(show_fps));
    app.manage(OpenMenuItem(open_item));
    app.manage(OpenTarget(std::sync::Mutex::new(None)));
    #[cfg(debug_assertions)]
    {
        let developer_tools =
            MenuItem::with_id(app, "open-dev-tools", "Developer Tools", true, None::<&str>)?;
        let prototype_switcher = CheckMenuItem::with_id(
            app,
            "toggle-prototype-switcher",
            "Show Prototype Switcher",
            true,
            false,
            None::<&str>,
        )?;
        let switcher_submenu =
            Submenu::with_items(app, "Prototype Switcher", true, &[&prototype_switcher])?;
        let debug_menu =
            Submenu::with_items(app, "Debug", true, &[&developer_tools, &switcher_submenu])?;
        menu.append(&debug_menu)?;
        app.manage(PrototypeSwitcherMenu(prototype_switcher));
    }
    order_top_level_menus(&menu)?;
    app.manage(info);
    Ok(menu)
}

/// Top-level menus in the order people expect. Tauri's default menu differs per platform (Linux
/// has no File or View menu, Windows no View menu), so menus added above land after Help.
const MENU_ORDER: &[&str] = &["File", "Edit", "View", "Window", "Help", "Debug"];

/// Sort key for a top-level menu. Menus outside `MENU_ORDER`, like the macOS app menu, stay first.
fn menu_rank(label: &str) -> usize {
    let label = strip_mnemonic(label);
    MENU_ORDER
        .iter()
        .position(|name| *name == label)
        .map_or(0, |index| index + 1)
}

fn order_top_level_menus<R: Runtime>(menu: &Menu<R>) -> tauri::Result<()> {
    let mut submenus = Vec::new();
    for item in menu.items()? {
        if let MenuItemKind::Submenu(submenu) = item {
            submenus.push((menu_rank(&submenu.text()?), submenu));
        }
    }
    if submenus.windows(2).all(|pair| pair[0].0 <= pair[1].0) {
        return Ok(());
    }
    submenus.sort_by_key(|(rank, _)| *rank);
    for (_, submenu) in &submenus {
        menu.remove(submenu)?;
    }
    for (_, submenu) in &submenus {
        menu.append(submenu)?;
    }
    Ok(())
}

pub fn handle(app: &AppHandle, id: &str) {
    #[cfg(debug_assertions)]
    {
        if id == "open-dev-tools" {
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }
            return;
        }
        if id == "toggle-prototype-switcher" {
            let visible = app
                .state::<PrototypeSwitcherMenu>()
                .0
                .is_checked()
                .unwrap_or(false);
            let _ = app.emit("dev-tools", visible);
            return;
        }
    }
    if id == "toggle-sidebar-floating" {
        let floating = app.state::<SidebarMenu>().0.is_checked().unwrap_or(false);
        let _ = app.emit("sidebar-floating", floating);
    } else if id == "toggle-hidden-files" {
        let show = app
            .state::<HiddenFilesMenu>()
            .0
            .is_checked()
            .unwrap_or(false);
        let _ = app.emit("show-hidden-files", show);
    } else if id == "toggle-show-fps" {
        let show = app.state::<ShowFpsMenu>().0.is_checked().unwrap_or(false);
        let _ = app.emit("show-fps", show);
    } else if id == "new-folder" {
        let _ = app.emit("request-create-folder", ());
    } else if id == "new-file" {
        let _ = app.emit("request-create-file", ());
    } else if id == "new-tab" {
        let _ = app.emit("tab-new", ());
    } else if id == "new-tab-other" {
        let _ = app.emit("tab-new-other", ());
    } else if id == "toggle-second-pane" {
        let _ = app.emit("pane-toggle", ());
    } else if id == "toggle-pane-orientation" {
        let _ = app.emit("pane-orientation", ());
    } else if id == "close-tab" {
        let _ = app.emit("tab-close", ());
    } else if id == "next-tab" {
        let _ = app.emit("tab-next", ());
    } else if id == "previous-tab" {
        let _ = app.emit("tab-prev", ());
    } else if id == "open" {
        let target = app.state::<OpenTarget>().0.lock().unwrap().clone();
        if let Some((path, _)) = target {
            let _ = app.emit("request-open", path);
        }
    } else if id == "go-to-folder" {
        let _ = app.emit("command-palette", ());
    } else if id == CHECK_FOR_UPDATES {
        let _ = app.emit("check-for-updates", ());
    }
}

fn remove_close_window<R: Runtime>(submenu: &Submenu<R>) -> tauri::Result<()> {
    for item in submenu.items()? {
        if let MenuItemKind::Predefined(predefined) = &item {
            let text = predefined.text()?.replace('&', "");
            if text == "Close Window" || text == "Close" {
                submenu.remove(predefined)?;
            }
        }
    }
    Ok(())
}

/// Matches the predefined About item.
fn is_about<R: Runtime>(item: &MenuItemKind<R>) -> bool {
    match item {
        MenuItemKind::Predefined(predefined) => predefined
            .text()
            .map(|text| predefined_kind(&text) == Some(PredefinedKind::About))
            .unwrap_or(false),
        _ => false,
    }
}

/// Rebuilds the default About item with the app icon, so the About panel shows it even when the
/// binary runs outside an app bundle (e.g. `tauri dev`), where macOS falls back to a folder icon.
fn set_about_icon<R: Runtime>(
    app: &AppHandle<R>,
    menu: &Menu<R>,
    info: &AboutInfo,
) -> tauri::Result<()> {
    let bundle = &app.config().bundle;
    let about = PredefinedMenuItem::about(
        app,
        None,
        Some(AboutMetadata {
            name: Some(info.name.clone()),
            version: Some(info.version.clone()),
            copyright: Some(info.copyright.clone()).filter(|copyright| !copyright.is_empty()),
            authors: bundle.publisher.clone().map(|publisher| vec![publisher]),
            // `credits` is the description on macOS; `comments` covers Windows and Linux.
            credits: Some(info.description.clone()),
            comments: Some(info.description.clone()),
            icon: Some(tauri::include_image!("./icons/icon.png")),
            ..Default::default()
        }),
    )?;
    for entry in menu.items()? {
        let MenuItemKind::Submenu(submenu) = entry else {
            continue;
        };
        let index = submenu.items()?.iter().position(is_about);
        if let Some(index) = index {
            submenu.remove_at(index)?;
            return submenu.insert(&about, index);
        }
    }
    Ok(())
}

/// Puts "Check for Updates…" right after About: in the app menu on macOS, in Help elsewhere.
fn add_check_for_updates<R: Runtime>(app: &AppHandle<R>, menu: &Menu<R>) -> tauri::Result<()> {
    let item = MenuItem::with_id(
        app,
        CHECK_FOR_UPDATES,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    for entry in menu.items()? {
        let MenuItemKind::Submenu(submenu) = entry else {
            continue;
        };
        let about = submenu.items()?.iter().position(is_about);
        if let Some(index) = about {
            return submenu.insert(&item, index + 1);
        }
    }
    let help = menu.items()?.into_iter().find_map(|entry| match entry {
        MenuItemKind::Submenu(submenu) if submenu.id() == HELP_SUBMENU_ID => Some(submenu),
        _ => None,
    });
    match help {
        Some(help) => help.append(&item),
        None => menu.append(&Submenu::with_items(app, "Help", true, &[&item])?),
    }
}

pub struct SidebarMenu(pub CheckMenuItem<Wry>);

#[tauri::command]
pub fn set_sidebar_floating(app: AppHandle, menu: State<'_, SidebarMenu>, floating: bool) {
    let _ = menu.0.set_checked(floating);
    let _ = app.emit(MENU_CHANGED, ());
}

pub struct HiddenFilesMenu(pub CheckMenuItem<Wry>);

#[tauri::command]
pub fn set_show_hidden_files(app: AppHandle, menu: State<'_, HiddenFilesMenu>, show: bool) {
    let _ = menu.0.set_checked(show);
    let _ = app.emit(MENU_CHANGED, ());
}

pub struct ShowFpsMenu(pub CheckMenuItem<Wry>);

#[tauri::command]
pub fn set_show_fps(app: AppHandle, menu: State<'_, ShowFpsMenu>, show: bool) {
    let _ = menu.0.set_checked(show);
    let _ = app.emit(MENU_CHANGED, ());
}

#[cfg(debug_assertions)]
pub struct PrototypeSwitcherMenu(pub CheckMenuItem<Wry>);

pub struct OpenMenuItem(pub MenuItem<Wry>);
pub struct OpenTarget(pub std::sync::Mutex<Option<(String, bool)>>);

#[tauri::command]
pub fn set_open_target(app: AppHandle, path: String, is_directory: bool, enabled: bool) {
    let open_menu = &app.state::<OpenMenuItem>().0;
    let text = if enabled {
        if is_directory {
            "Open Folder"
        } else {
            "Open File"
        }
    } else {
        "Open"
    };
    let _ = open_menu.set_text(text);
    let _ = open_menu.set_enabled(enabled);
    *app.state::<OpenTarget>().0.lock().unwrap() = if enabled && !path.is_empty() {
        Some((path, is_directory))
    } else {
        None
    };
    let _ = app.emit(MENU_CHANGED, ());
}

pub const MENU_CHANGED: &str = "menu-changed";

/// The app menu. macOS attaches it natively; Windows and Linux render it in the title bar.
pub struct AppMenu(pub Menu<Wry>);

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MenuNode {
    Submenu {
        label: String,
        children: Vec<MenuNode>,
    },
    Item {
        id: String,
        label: String,
        accelerator: Option<String>,
        enabled: bool,
    },
    Check {
        id: String,
        label: String,
        accelerator: Option<String>,
        enabled: bool,
        checked: bool,
    },
    Separator,
    Predefined {
        kind: PredefinedKind,
        label: String,
        accelerator: Option<String>,
        info: Option<AboutInfo>,
    },
}

/// muda's accelerators for predefined items on Windows and Linux.
fn predefined_accelerator(kind: PredefinedKind) -> Option<&'static str> {
    match kind {
        PredefinedKind::Undo => Some("CmdOrCtrl+Z"),
        PredefinedKind::Redo => Some("CmdOrCtrl+Y"),
        PredefinedKind::Cut => Some("CmdOrCtrl+X"),
        PredefinedKind::Copy => Some("CmdOrCtrl+C"),
        PredefinedKind::Paste => Some("CmdOrCtrl+V"),
        PredefinedKind::SelectAll => Some("CmdOrCtrl+A"),
        PredefinedKind::Minimize => Some("CmdOrCtrl+M"),
        _ => None,
    }
}

fn menu_nodes<R: Runtime>(
    items: Vec<MenuItemKind<R>>,
    about: &AboutInfo,
) -> tauri::Result<Vec<MenuNode>> {
    let mut nodes = Vec::new();
    for item in items {
        match item {
            MenuItemKind::Submenu(submenu) => nodes.push(MenuNode::Submenu {
                label: strip_mnemonic(&submenu.text()?),
                children: menu_nodes(submenu.items()?, about)?,
            }),
            MenuItemKind::MenuItem(item) => {
                let id = item.id().as_ref().to_string();
                nodes.push(MenuNode::Item {
                    accelerator: accelerator(&id).map(str::to_string),
                    label: strip_mnemonic(&item.text()?),
                    enabled: item.is_enabled()?,
                    id,
                });
            }
            MenuItemKind::Check(item) => {
                let id = item.id().as_ref().to_string();
                nodes.push(MenuNode::Check {
                    accelerator: accelerator(&id).map(str::to_string),
                    label: strip_mnemonic(&item.text()?),
                    enabled: item.is_enabled()?,
                    checked: item.is_checked()?,
                    id,
                });
            }
            MenuItemKind::Predefined(item) => {
                let text = item.text()?;
                if text.is_empty() {
                    nodes.push(MenuNode::Separator);
                } else if let Some(kind) = predefined_kind(&text) {
                    let about_item = kind == PredefinedKind::About;
                    nodes.push(MenuNode::Predefined {
                        kind,
                        label: if about_item {
                            format!("About {}", about.name)
                        } else {
                            strip_mnemonic(&text)
                        },
                        accelerator: predefined_accelerator(kind).map(str::to_string),
                        info: about_item.then(|| about.clone()),
                    });
                }
            }
            MenuItemKind::Icon(_) => {}
        }
    }
    Ok(nodes)
}

/// Finds an item anywhere in the tree; `Menu::get` only searches the top level.
fn find_item<R: Runtime>(items: Vec<MenuItemKind<R>>, id: &str) -> Option<MenuItemKind<R>> {
    for item in items {
        if item.id().as_ref() == id {
            return Some(item);
        }
        if let MenuItemKind::Submenu(submenu) = &item {
            if let Some(found) = submenu
                .items()
                .ok()
                .and_then(|children| find_item(children, id))
            {
                return Some(found);
            }
        }
    }
    None
}

#[tauri::command]
pub fn app_menu(
    menu: State<'_, AppMenu>,
    about: State<'_, AboutInfo>,
) -> Result<Vec<MenuNode>, String> {
    menu.0
        .items()
        .and_then(|items| menu_nodes(items, about.inner()))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn trigger_menu(app: AppHandle, menu: State<'_, AppMenu>, id: String) -> Result<(), String> {
    let items = menu.0.items().map_err(|error| error.to_string())?;
    let item = find_item(items, &id).ok_or_else(|| format!("Unknown menu item: {id}"))?;
    // A native check item flips itself before its event; do the same for title bar clicks.
    if let MenuItemKind::Check(check) = &item {
        let checked = check.is_checked().map_err(|error| error.to_string())?;
        check
            .set_checked(!checked)
            .map_err(|error| error.to_string())?;
        let _ = app.emit(MENU_CHANGED, ());
    }
    handle(&app, &id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_mnemonic_removes_markers_and_keeps_escaped_ampersands() {
        assert_eq!(strip_mnemonic("&About"), "About");
        assert_eq!(strip_mnemonic("Cu&t"), "Cut");
        assert_eq!(strip_mnemonic("Save && Quit"), "Save & Quit");
        assert_eq!(strip_mnemonic("About Lite Explorer"), "About Lite Explorer");
    }

    #[test]
    fn predefined_kind_matches_default_labels_on_every_platform() {
        assert_eq!(predefined_kind("&About"), Some(PredefinedKind::About));
        assert_eq!(
            predefined_kind("About Lite Explorer"),
            Some(PredefinedKind::About)
        );
        assert_eq!(predefined_kind("&Copy"), Some(PredefinedKind::Copy));
        assert_eq!(predefined_kind("Cu&t"), Some(PredefinedKind::Cut));
        assert_eq!(predefined_kind("&Paste"), Some(PredefinedKind::Paste));
        assert_eq!(
            predefined_kind("Select &All"),
            Some(PredefinedKind::SelectAll)
        );
        assert_eq!(predefined_kind("Undo"), Some(PredefinedKind::Undo));
        assert_eq!(predefined_kind("Redo"), Some(PredefinedKind::Redo));
        assert_eq!(predefined_kind("&Minimize"), Some(PredefinedKind::Minimize));
        assert_eq!(predefined_kind("Ma&ximize"), Some(PredefinedKind::Maximize));
        assert_eq!(predefined_kind("Close"), Some(PredefinedKind::CloseWindow));
        assert_eq!(
            predefined_kind("C&lose Window"),
            Some(PredefinedKind::CloseWindow)
        );
        assert_eq!(predefined_kind("&Exit"), Some(PredefinedKind::Quit));
        assert_eq!(predefined_kind("&Quit"), Some(PredefinedKind::Quit));
        assert_eq!(
            predefined_kind("Toggle Full Screen"),
            Some(PredefinedKind::Fullscreen)
        );
        assert_eq!(predefined_kind("Services"), None);
        assert_eq!(predefined_kind(""), None);
    }

    #[test]
    fn menu_rank_orders_file_edit_view_window_help_debug() {
        let mut labels = vec![
            "Edit",
            "Window",
            "&Help",
            "File",
            "View",
            "Debug",
            "Lite Explorer",
        ];
        labels.sort_by_key(|label| menu_rank(label));
        assert_eq!(
            labels,
            vec![
                "Lite Explorer",
                "File",
                "Edit",
                "View",
                "Window",
                "&Help",
                "Debug"
            ]
        );
    }

    #[test]
    fn accelerator_looks_up_items_by_id() {
        assert_eq!(accelerator("new-tab"), Some("CmdOrCtrl+T"));
        assert_eq!(
            accelerator("toggle-hidden-files"),
            Some("CmdOrCtrl+Shift+Period")
        );
        assert_eq!(accelerator("toggle-show-fps"), None);
    }

    #[test]
    fn menu_nodes_serialize_to_the_frontend_shape() {
        let nodes = vec![MenuNode::Submenu {
            label: "View".to_string(),
            children: vec![
                MenuNode::Check {
                    id: "toggle-hidden-files".to_string(),
                    label: "Show Hidden Files".to_string(),
                    accelerator: Some("CmdOrCtrl+Shift+Period".to_string()),
                    enabled: true,
                    checked: false,
                },
                MenuNode::Separator,
                MenuNode::Item {
                    id: "open".to_string(),
                    label: "Open".to_string(),
                    accelerator: None,
                    enabled: false,
                },
                MenuNode::Predefined {
                    kind: PredefinedKind::SelectAll,
                    label: "Select All".to_string(),
                    accelerator: Some("CmdOrCtrl+A".to_string()),
                    info: None,
                },
            ],
        }];
        assert_eq!(
            serde_json::to_value(&nodes).unwrap(),
            serde_json::json!([{
                "type": "submenu",
                "label": "View",
                "children": [
                    { "type": "check", "id": "toggle-hidden-files", "label": "Show Hidden Files",
                      "accelerator": "CmdOrCtrl+Shift+Period", "enabled": true, "checked": false },
                    { "type": "separator" },
                    { "type": "item", "id": "open", "label": "Open", "accelerator": null, "enabled": false },
                    { "type": "predefined", "kind": "selectAll", "label": "Select All",
                      "accelerator": "CmdOrCtrl+A", "info": null }
                ]
            }])
        );
    }

    #[test]
    fn predefined_accelerators_match_muda_defaults_outside_macos() {
        assert_eq!(
            predefined_accelerator(PredefinedKind::Copy),
            Some("CmdOrCtrl+C")
        );
        assert_eq!(
            predefined_accelerator(PredefinedKind::Redo),
            Some("CmdOrCtrl+Y")
        );
        assert_eq!(
            predefined_accelerator(PredefinedKind::Minimize),
            Some("CmdOrCtrl+M")
        );
        assert_eq!(predefined_accelerator(PredefinedKind::About), None);
    }
}
