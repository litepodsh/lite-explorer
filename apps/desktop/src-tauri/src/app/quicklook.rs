//! macOS Quick Look: the system preview panel Finder opens with Space. It renders every
//! format the system or an installed Quick Look extension knows (Keynote, Pages, HEIC,
//! RAW photos, USDZ, fonts...), so it covers what the built-in preview does not.
//!
//! `QLPreviewPanel` asks the responder chain of the key window for a controller. Tao's
//! window class gets the three controller methods added at runtime, so the panel accepts the
//! app window and takes its items from [`QuickLookSource`]. The panel is ordered front
//! without becoming key: arrow keys keep moving the selection in the webview, which sends
//! the new items with `quick_look_update`, like Finder.

// The panel exists on macOS only; elsewhere the commands report that.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::sync::{Mutex, OnceLock};

use tauri::AppHandle;

/// Paths the panel shows, in selection order.
static ITEMS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static APP: OnceLock<AppHandle> = OnceLock::new();

fn set_items(paths: Vec<String>) {
    if let Ok(mut items) = ITEMS.lock() {
        *items = paths;
    }
}

fn items() -> Vec<String> {
    ITEMS.lock().map(|items| items.clone()).unwrap_or_default()
}

/// Key the frontend should act on, for keys pressed while the panel itself is focused.
fn forwarded_key(key_code: u16, items: usize) -> Option<&'static str> {
    // With several items, the panel's own arrows page through them.
    match key_code {
        49 => Some(" "),
        125 if items <= 1 => Some("ArrowDown"),
        126 if items <= 1 => Some("ArrowUp"),
        123 if items <= 1 => Some("ArrowLeft"),
        124 if items <= 1 => Some("ArrowRight"),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
mod native {
    use std::cell::OnceCell;

    use objc2::{
        define_class, msg_send,
        rc::Retained,
        runtime::{AnyClass, AnyObject, Bool, Imp, NSObjectProtocol, ProtocolObject, Sel},
        sel, MainThreadMarker, MainThreadOnly,
    };
    use objc2_app_kit::{NSEvent, NSEventType, NSWindowDelegate};
    use objc2_foundation::{NSInteger, NSObject, NSString, NSURL};
    use objc2_quick_look_ui::{
        QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate,
    };
    use tauri::Emitter;

    use super::{forwarded_key, items, APP};

    define_class!(
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "LiteExplorerQuickLookSource"]
        pub struct QuickLookSource;

        unsafe impl NSObjectProtocol for QuickLookSource {}

        unsafe impl QLPreviewPanelDataSource for QuickLookSource {
            #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
            fn number_of_items(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
                items().len() as NSInteger
            }

            #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
            fn item(
                &self,
                _panel: Option<&QLPreviewPanel>,
                index: NSInteger,
            ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
                preview_item(index)
            }
        }

        unsafe impl NSWindowDelegate for QuickLookSource {}

        unsafe impl QLPreviewPanelDelegate for QuickLookSource {
            /// Keys pressed after clicking into the panel go back to the file list.
            #[unsafe(method(previewPanel:handleEvent:))]
            fn handle_event(&self, _panel: Option<&QLPreviewPanel>, event: Option<&NSEvent>) -> Bool {
                Bool::new(event.is_some_and(forward_key))
            }
        }
    );

    fn preview_item(index: NSInteger) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
        let path = items().get(usize::try_from(index).ok()?)?.clone();
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
        Some(ProtocolObject::from_retained(url))
    }

    /// Sends a key pressed in the panel to the frontend. False leaves it to the panel.
    fn forward_key(event: &NSEvent) -> bool {
        if event.r#type() != NSEventType::KeyDown {
            return false;
        }
        let Some(key) = forwarded_key(event.keyCode(), items().len()) else {
            return false;
        };
        if let Some(app) = APP.get() {
            let _ = app.emit("quick-look-key", key);
        }
        true
    }

    thread_local! {
        /// The panel keeps its data source unretained, so the one instance lives here.
        static SOURCE: OnceCell<Retained<QuickLookSource>> = const { OnceCell::new() };
    }

    fn source(mtm: MainThreadMarker) -> Retained<QuickLookSource> {
        SOURCE.with(|cell| {
            cell.get_or_init(|| unsafe { msg_send![QuickLookSource::alloc(mtm), init] })
                .clone()
        })
    }

    unsafe extern "C-unwind" fn accepts_control(
        _this: &AnyObject,
        _cmd: Sel,
        _panel: *mut AnyObject,
    ) -> Bool {
        Bool::YES
    }

    unsafe extern "C-unwind" fn begin_control(_this: &AnyObject, _cmd: Sel, panel: *mut AnyObject) {
        let (Some(mtm), Some(panel)) = (MainThreadMarker::new(), panel.cast::<QLPreviewPanel>().as_ref())
        else {
            return;
        };
        attach(mtm, panel);
        panel.reloadData();
    }

    /// Points the panel at [`QuickLookSource`].
    unsafe fn attach(mtm: MainThreadMarker, panel: &QLPreviewPanel) {
        let source = source(mtm);
        panel.setDataSource(Some(ProtocolObject::from_ref(&*source)));
        panel.setDelegate(Some(&source));
    }

    unsafe extern "C-unwind" fn end_control(_this: &AnyObject, _cmd: Sel, panel: *mut AnyObject) {
        if let Some(panel) = panel.cast::<QLPreviewPanel>().as_ref() {
            panel.setDataSource(None);
            panel.setDelegate(None);
        }
        if let Some(app) = APP.get() {
            let _ = app.emit("quick-look-closed", ());
        }
    }

    /// Adds the controller methods to the window's class once. A class that already has them
    /// keeps its own (`class_addMethod` refuses to replace).
    unsafe fn install_controller(window: *mut AnyObject) {
        let class = objc2::ffi::object_getClass(window) as *mut AnyClass;
        if class.is_null() {
            return;
        }
        #[cfg(target_arch = "aarch64")]
        let accepts_types = c"B@:@";
        #[cfg(not(target_arch = "aarch64"))]
        let accepts_types = c"c@:@";
        type Accepts = unsafe extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject) -> Bool;
        type Control = unsafe extern "C-unwind" fn(&AnyObject, Sel, *mut AnyObject);
        let methods: [(Sel, Imp, &std::ffi::CStr); 3] = [
            (
                sel!(acceptsPreviewPanelControl:),
                std::mem::transmute::<Accepts, Imp>(accepts_control),
                accepts_types,
            ),
            (
                sel!(beginPreviewPanelControl:),
                std::mem::transmute::<Control, Imp>(begin_control),
                c"v@:@",
            ),
            (
                sel!(endPreviewPanelControl:),
                std::mem::transmute::<Control, Imp>(end_control),
                c"v@:@",
            ),
        ];
        for (name, imp, types) in methods {
            objc2::ffi::class_addMethod(class, name, imp, types.as_ptr());
        }
    }

    fn shared_panel(mtm: MainThreadMarker) -> Option<Retained<QLPreviewPanel>> {
        unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }
    }

    /// Shows the panel for the current items, or closes it when it is already showing.
    pub fn toggle(window: *mut AnyObject, index: usize) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(panel) = shared_panel(mtm) else { return };
        unsafe {
            if panel.isVisible() {
                panel.orderOut(None);
                return;
            }
            install_controller(window);
            // The panel looks for its controller as it appears, so it is shown first and
            // loaded after; loading before showing left it on "No selection".
            panel.orderFront(None);
            panel.updateController();
            // No controller found in the responder chain: feed the panel directly.
            if panel.currentController().is_none() || panel.dataSource().is_none() {
                attach(mtm, &panel);
            }
            panel.reloadData();
            panel.setCurrentPreviewItemIndex(index as NSInteger);
        }
    }

    /// Points an open panel at the new items; a closed one is left closed.
    pub fn update(index: usize) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if !unsafe { QLPreviewPanel::sharedPreviewPanelExists(mtm) } {
            return;
        }
        let Some(panel) = shared_panel(mtm) else { return };
        unsafe {
            if !panel.isVisible() {
                return;
            }
            if panel.dataSource().is_none() {
                attach(mtm, &panel);
            }
            panel.reloadData();
            panel.setCurrentPreviewItemIndex(index as NSInteger);
        }
    }

    pub fn close() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        if unsafe { QLPreviewPanel::sharedPreviewPanelExists(mtm) } {
            if let Some(panel) = shared_panel(mtm) {
                if panel.isVisible() {
                    panel.orderOut(None);
                }
            }
        }
    }
}

/// Opens Quick Look on `paths`, starting at `index`, or closes it when it is open.
#[tauri::command]
pub fn quick_look(
    app: AppHandle,
    window: tauri::Window,
    paths: Vec<String>,
    index: usize,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = APP.set(app.clone());
        let ns_window = window.ns_window().map_err(|error| error.to_string())? as usize;
        set_items(paths);
        app.run_on_main_thread(move || native::toggle(ns_window as *mut _, index))
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, window, paths, index);
        Err("Quick Look is only available on macOS".into())
    }
}

/// Follows the selection while the panel is open. Does nothing when it is closed.
#[tauri::command]
pub fn quick_look_update(app: AppHandle, paths: Vec<String>, index: usize) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        set_items(paths);
        app.run_on_main_thread(move || native::update(index))
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, paths, index);
        Ok(())
    }
}

#[tauri::command]
pub fn quick_look_close(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        app.run_on_main_thread(native::close)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_go_to_the_list_only_for_a_single_item() {
        assert_eq!(forwarded_key(125, 1), Some("ArrowDown"));
        assert_eq!(forwarded_key(125, 3), None);
        assert_eq!(forwarded_key(49, 3), Some(" "));
        assert_eq!(forwarded_key(0, 1), None);
    }
}
