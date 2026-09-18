mod ai;
mod app;
mod explorer;
mod media;
mod network;
mod preview;
mod remote;
mod search;
mod system;

pub(crate) use app::db::Database;
pub(crate) use explorer::entries::{
    epoch_millis, single_entry, unique_name, utf8_boundary, DirectoryEntry,
};
pub(crate) use explorer::paths::{now_secs, Location};
pub(crate) use preview::{
    image_mime, media_mime, media_preview_kind, FilePreview, PreviewKind, PREVIEW_MAX_BYTES,
    PREVIEW_SNIFF_BYTES,
};

use app::db::open_database;
use app::swipe_nav;
#[cfg(target_os = "macos")]
use app::window::unlock_webview_frame_rate;
use app::window::{restore_window_state, save_window_state};
use system::folder_usage::FolderScans;
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let analytics = app::analytics::Analytics::load();
    let sentry_client = app::analytics::init_sentry(analytics.gate(), &analytics.install_id());
    app::analytics::init_minidump(&sentry_client, analytics.enabled());
    app::analytics::capture_first_run(analytics.prefs().welcome_seen);
    let mut builder = media::register(tauri::Builder::default())
        .plugin(tauri_plugin_sentry::init(&sentry_client))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());
    // Apple Intelligence is macOS 26+ on Apple silicon only; the crate's prebuilt dylib is arm64
    // and would not link for any other target.
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        builder = builder.plugin(tauri_plugin_apple_intelligence::init());
    }
    builder
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                restore_window_state(&window);
                let window = window.clone();
                window.clone().on_window_event(move |event| {
                    if matches!(event, WindowEvent::CloseRequested { .. }) {
                        save_window_state(&window);
                    }
                });
            }
            app.manage(analytics);
            app.manage(media::MediaRegistry::default());
            app.manage(media::viewer::ViewerTarget::default());
            let database = tauri::async_runtime::block_on(open_database(app.handle()))?;
            app.manage(Database(database));
            app.manage(remote::RemoteClients::default());
            app.manage(network::Mounts::default());
            app.manage(network::discovery::Discovery::default());
            app.manage(network::servers::Sessions::default());
            app.manage(remote::transfer::TransferRegistry::default());
            app.manage(FolderScans(std::sync::Mutex::new(
                std::collections::HashSet::new(),
            )));
            app.manage(swipe_nav::SwipeState::default());
            let menu = app::menu::build(app.handle())?;
            app.manage(app::menu::AppMenu(menu.clone()));
            #[cfg(target_os = "macos")]
            app.set_menu(menu)?;
            #[cfg(target_os = "macos")]
            for window in app.webview_windows().values() {
                unlock_webview_frame_rate(window);
            }
            swipe_nav::init(app.handle());
            Ok(())
        })
        .on_menu_event(|app, event| app::menu::handle(app, event.id().as_ref()))
        .invoke_handler(tauri::generate_handler![
            app::general::os_detection,
            ai::apple_intelligence_status,
            ai::suggest_name,
            app::analytics::analytics_prefs,
            app::analytics::save_analytics,
            app::icons::file_icons,
            app::download_progress::read_download_progress,
            app::menu::set_show_hidden_files,
            app::menu::set_show_fps,
            app::menu::set_prototype_switcher,
            app::menu::open_settings,
            explorer::paths::locations,
            remote::test_remote_location,
            remote::add_remote_location,
            remote::remove_remote_location,
            network::test_network_location,
            network::add_network_location,
            network::update_network_location,
            network::remove_network_location,
            network::network_location,
            network::connect_network_location,
            network::disconnect_network_location,
            network::network_connections,
            network::mount_network_share,
            network::trust_network_host,
            network::discovery::scan_smb_servers,
            network::discovery::stop_network_scan,
            network::discovery::open_local_network_settings,
            remote::download_remote_file,
            remote::write::delete_remote_items,
            remote::write::upload_remote_files,
            remote::write::download_remote_items,
            remote::transfer::cancel_transfer,
            explorer::archive::create_archive,
            explorer::archive::extract_archive,
            explorer::archive::list_archive,
            explorer::archive::plan_extraction,
            remote::buckets::create_remote_bucket,
            remote::buckets::delete_remote_bucket,
            remote::buckets::remote_provider,
            remote::bucket_settings::bucket_settings,
            remote::bucket_settings::set_bucket_versioning,
            remote::bucket_settings::set_bucket_public,
            explorer::favorites::favorites,
            explorer::favorites::add_favorite,
            explorer::favorites::remove_favorite,
            explorer::favorites::reorder_favorites,
            explorer::entries::read_directory,
            explorer::entries::resolve_path,
            explorer::entries::search_directory,
            explorer::file_ops::create_item,
            explorer::file_ops::rename_item,
            explorer::opener::open_with_apps,
            explorer::opener::open_with,
            explorer::opener::open_path,
            explorer::opener::open_terminal,
            explorer::opener::detect_terminals,
            explorer::opener::reveal_path,
            explorer::opener::default_app,
            app::menu::set_open_target,
            app::menu::app_menu,
            app::menu::trigger_menu,
            app::swipe_nav::set_swipe_context,
            media::media_url,
            media::viewer::open_viewer,
            media::viewer::viewer_target,
            explorer::sizes::compute_directory_sizes,
            explorer::sizes::scan_directory_sizes,
            explorer::sizes::cancel_directory_size_scan,
            preview::read_file_preview,
            explorer::recents::record_recent,
            explorer::recents::recents,
            explorer::recents::clear_recents,
            explorer::file_ops::copy_item,
            explorer::file_ops::move_item,
            explorer::file_ops::trash_item,
            explorer::file_ops::delete_item,
            system::volumes::disk_overview,
            system::folder_usage::folder_usage,
            system::folder_usage::scan_folder_usage,
            system::trash::empty_trash,
            system::trash::open_system_settings,
            system::trash::request_full_disk_access,
            system::trash::open_trash,
            system::trash::trash_usage
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
