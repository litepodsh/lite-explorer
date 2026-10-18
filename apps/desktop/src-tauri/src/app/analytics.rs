//! Anonymous error reporting (Sentry), usage analytics (Aptabase) and their shared
//! opt-out preference.
//!
//! Reporting is on by default. The preference lives in a small JSON file so the Rust
//! side can read it *before* the Sentry client is created — the frontend preference
//! store cannot be reached that early. The file uses the same layout Tauri uses for
//! `app_config_dir`, so it sits next to the rest of the app's data.
//!
//! Before 1.0 the toggle is locked on: `save_analytics` keeps it enabled no matter
//! what the UI sends.

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{ipc::Invoke, AppHandle, Manager, Runtime, State};
use tauri_plugin_aptabase::{EventTracker, InitOptions};
use uuid::Uuid;

/// Matches `identifier` in `tauri.conf.json`.
const IDENTIFIER: &str = "xyz.sebasgc.liteexplorer";
const PREFS_FILE: &str = "analytics.json";

/// Share of performance transactions sent to Sentry. Sampling happens when a root
/// `tracing` span starts, so 10% keeps the volume (and quota) low while still giving a
/// usable picture of slow commands.
const TRACES_SAMPLE_RATE: f32 = 0.1;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AnalyticsPrefs {
    /// Anonymous crash and error reports. On by default.
    pub enabled: bool,
    /// Whether the first-run welcome has already been shown.
    pub welcome_seen: bool,
    /// Random id for this install. Sent as the Sentry `user.id` so Sentry can count
    /// unique installs. Not tied to any account, hardware or path.
    pub install_id: String,
}

impl Default for AnalyticsPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            welcome_seen: false,
            install_id: Uuid::now_v7().to_string(),
        }
    }
}

pub struct Analytics {
    gate: Arc<AtomicBool>,
    prefs: Mutex<AnalyticsPrefs>,
    path: PathBuf,
    /// Whether the Aptabase plugin is registered. Without an app key it is not, and its
    /// client state does not exist, so tracking must not touch it.
    usage: bool,
}

impl Analytics {
    /// Reads the saved preference, or the default (enabled) on a first run.
    pub fn load() -> Self {
        let path = prefs_path();
        let mut prefs = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<AnalyticsPrefs>(&bytes).ok())
            .unwrap_or_default();
        // Files written before the id existed deserialize with an empty one.
        let is_new = prefs.install_id.is_empty();
        if is_new {
            prefs.install_id = Uuid::now_v7().to_string();
        }
        eprintln!(
            "[analytics] preference file: {} (enabled: {}, install: {})",
            path.display(),
            prefs.enabled,
            prefs.install_id
        );
        let analytics = Self {
            gate: Arc::new(AtomicBool::new(prefs.enabled)),
            prefs: Mutex::new(prefs.clone()),
            path,
            usage: aptabase_key().is_some(),
        };
        // The id must survive the very first run, otherwise every launch counts as new.
        if is_new {
            analytics.store(prefs);
        }
        analytics
    }

    /// Shared "is reporting enabled" flag read by the Sentry hooks. Flipped live.
    pub fn gate(&self) -> Arc<AtomicBool> {
        self.gate.clone()
    }

    pub fn enabled(&self) -> bool {
        self.gate.load(Ordering::Relaxed)
    }

    pub fn prefs(&self) -> AnalyticsPrefs {
        self.lock().clone()
    }

    /// Anonymous per-install id, minted on first run and stable afterwards.
    pub fn install_id(&self) -> String {
        self.lock().install_id.clone()
    }

    /// Sends a usage event to Aptabase. Dropped when reporting is off or no app key was
    /// baked in. Aptabase only takes string and number property values.
    pub fn track(&self, app: &AppHandle, name: &str, props: Option<Value>) {
        if !self.usage || !self.enabled() {
            return;
        }
        if let Err(error) = app.track_event(name, props) {
            eprintln!("[analytics] couldn't track {name}: {error}");
        }
    }

    /// Debug menu check: sends a `debug_test_event` to Aptabase and describes what happened,
    /// so a build can be checked for a working app key without waiting for real usage.
    pub fn send_test_event(&self, app: &AppHandle) -> String {
        let install = self.install_id();
        if !self.usage {
            return format!(
                "Not sent: no Aptabase app key was baked into this build.\n\nInstall: {install}"
            );
        }
        if !self.enabled() {
            return format!("Not sent: reporting is turned off.\n\nInstall: {install}");
        }
        let props = serde_json::json!({ "source": "debug-menu" });
        match app.track_event("debug_test_event", Some(props)) {
            Ok(()) => format!(
                "Queued \"debug_test_event\" for Aptabase. It is flushed within a few seconds; look for it in the dashboard.\n\nInstall: {install}"
            ),
            Err(error) => format!("Couldn't send: {error}\n\nInstall: {install}"),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, AnalyticsPrefs> {
        self.prefs.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn store(&self, prefs: AnalyticsPrefs) {
        self.gate.store(prefs.enabled, Ordering::Relaxed);
        *self.lock() = prefs.clone();
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec_pretty(&prefs) {
            let _ = fs::write(&self.path, bytes);
        }
    }
}

/// `app_config_dir` equivalent, resolved without an `AppHandle` so it can run before
/// the Tauri app exists.
fn prefs_path() -> PathBuf {
    let base = config_base().unwrap_or_else(|| PathBuf::from("."));
    base.join(IDENTIFIER).join(PREFS_FILE)
}

fn config_base() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// The DSN baked in at build time. Parsed up front because `sentry::init` panics on a
/// malformed DSN and this build aborts on panics — a bad `.env` must not stop the app.
fn dsn() -> Option<sentry::types::Dsn> {
    option_env!("SENTRY_DSN")
        .map(str::trim)
        .filter(|dsn| !dsn.is_empty())
        .and_then(|dsn| match dsn.parse() {
            Ok(dsn) => Some(dsn),
            Err(_) => {
                eprintln!("[liteexplorer] ignoring invalid SENTRY_DSN");
                None
            }
        })
}

/// Creates the Sentry client. The returned guard must stay alive for the app's lifetime.
/// Without a DSN (no `.env`, no CI secret) the client is created disabled and captures
/// nothing. An unreachable server only stalls the background transport, never startup.
///
/// `install_id` is attached as the Sentry `user.id`: that is what turns events and
/// sessions into a unique-user count in the dashboard.
pub fn init_sentry(gate: Arc<AtomicBool>, install_id: &str) -> sentry::ClientInitGuard {
    let before_send_gate = gate.clone();
    // Development builds never report: debug noise stays on the machine.
    let parsed_dsn = if cfg!(debug_assertions) { None } else { dsn() };
    if cfg!(debug_assertions) {
        eprintln!("[analytics] development build; Sentry reporting disabled");
    } else {
        match &parsed_dsn {
            Some(dsn) => eprintln!(
                "[analytics] Sentry enabled for {} project {}",
                dsn.host(),
                dsn.project_id()
            ),
            None => eprintln!("[analytics] SENTRY_DSN missing or invalid; reporting disabled"),
        }
    }
    // `ClientOptions` is `#[non_exhaustive]`, so it can only be built field by field.
    let mut options = sentry::ClientOptions::default();
    options.dsn = parsed_dsn;
    options.release = sentry::release_name!();
    options.environment = Some(
        if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        }
        .into(),
    );
    // Release health: sessions are what Sentry turns into a Users/Sessions count.
    // They bypass `before_send`, so they start only when reporting is on and are
    // stopped by hand in `save_analytics` when the user opts out.
    options.auto_session_tracking = gate.load(Ordering::Relaxed) && !cfg!(debug_assertions);
    // Performance: sample a share of transactions. Root `tracing` spans (registered via
    // the subscriber in `run`) become transactions; without a sample rate they are
    // dropped. Debug builds stay at the SDK default (off).
    if !cfg!(debug_assertions) {
        options = options.traces_sample_rate(TRACES_SAMPLE_RATE);
    }
    // Both the Rust SDK and the browser events forwarded by the Tauri plugin run
    // through these hooks, so flipping the gate stops reports immediately.
    options.before_send = Some(Arc::new(move |event| {
        let enabled = before_send_gate.load(Ordering::Relaxed) && !cfg!(debug_assertions);
        // Confirms an event reached the Rust client (UI errors arrive here too).
        eprintln!(
            "[analytics] event captured (enabled={enabled}, level={:?}, platform={}): {}",
            event.level,
            event.platform,
            event.message.as_deref().unwrap_or("<no message>")
        );
        // Monaco rejects cancelled work with a `Canceled` error; that is expected, not a bug.
        let monaco_cancel = event
            .exception
            .values
            .iter()
            .any(|exception| exception.ty == "Canceled");
        (enabled && !monaco_cancel).then_some(event)
    }));
    options.before_breadcrumb = Some(Arc::new(move |breadcrumb| {
        (gate.load(Ordering::Relaxed) && !cfg!(debug_assertions)).then_some(breadcrumb)
    }));
    let guard = sentry::init(options);
    sentry::configure_scope(|scope| {
        scope.set_user(Some(sentry::protocol::User {
            id: Some(install_id.to_string()),
            ..Default::default()
        }));
    });
    guard
}

/// Registers the global `tracing` subscriber with the Sentry layer. Instrumented
/// functions are root spans, which become Sentry performance transactions; the fmt
/// layer keeps local console output. Must run after `init_sentry` so the current hub
/// exists. Calling it twice (another subscriber already installed) is ignored.
pub fn init_tracing() {
    use tracing_subscriber::prelude::*;

    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(sentry::integrations::tracing::layer());
    if subscriber.try_init().is_err() {
        eprintln!("[analytics] tracing subscriber already installed");
    }
}

/// One event per fresh install, so "new installs" can be split from returning users.
/// Dropped by the gate like any other event when reporting is off.
pub fn capture_first_run(welcome_seen: bool) {
    if welcome_seen {
        return;
    }
    sentry::with_scope(
        |scope| scope.set_tag("install", "first_run"),
        || {
            sentry::capture_message("first_run", sentry::Level::Info);
        },
    );
}

/// Starts the native-crash reporter process. Skipped when reporting is off or there is
/// no DSN. Any failure to spawn is logged and ignored so startup always continues. On
/// the crash-reporter process this call never returns.
pub fn init_minidump(client: &sentry::Client, enabled: bool) {
    if !enabled || !client.is_enabled() {
        return;
    }
    match tauri_plugin_sentry::minidump::init(client) {
        // The handle keeps the reporter attached; it must live for the whole process.
        Ok(handle) => std::mem::forget(handle),
        Err(error) => eprintln!("[liteexplorer] crash reporter unavailable: {error}"),
    }
}

/// Aptabase app key baked in at build time (`A-EU-…`, `A-US-…` or `A-SH-…`).
fn aptabase_key() -> Option<&'static str> {
    option_env!("APTABASE_APP_KEY")
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

/// Server for a self-hosted (`A-SH-…`) key, baked in at build time. Cloud keys pick
/// their region from the key and ignore it.
fn aptabase_host() -> Option<String> {
    option_env!("APTABASE_HOST")
        .map(|host| host.trim().trim_end_matches('/'))
        .filter(|host| !host.is_empty())
        .map(str::to_owned)
}

/// The Aptabase plugin, or `None` without an app key. Crash reporting stays with Sentry,
/// so the plugin's own panic hook is not installed. Debug builds still send events:
/// Aptabase flags them and keeps them apart from release data in the dashboard.
pub fn aptabase_plugin<R: Runtime>() -> Option<tauri::plugin::TauriPlugin<R>> {
    let Some(key) = aptabase_key() else {
        eprintln!("[analytics] APTABASE_APP_KEY missing; usage analytics disabled");
        return None;
    };
    let host = aptabase_host();
    if key.starts_with("A-SH-") && host.is_none() {
        eprintln!(
            "[analytics] APTABASE_HOST missing for a self-hosted key; usage analytics disabled"
        );
        return None;
    }
    eprintln!(
        "[analytics] Aptabase enabled ({})",
        host.as_deref().unwrap_or("cloud")
    );
    Some(
        tauri_plugin_aptabase::Builder::new(key)
            .with_options(InitOptions {
                host,
                flush_interval: None,
            })
            .build(),
    )
}

/// Startup events: one `app_started` per launch, plus `first_run` on a fresh install.
pub fn track_launch(app: &AppHandle) {
    let Some(analytics) = app.try_state::<Analytics>() else {
        return;
    };
    analytics.track(app, "app_started", None);
    if !analytics.prefs().welcome_seen {
        analytics.track(app, "first_run", None);
    }
}

/// Usage event for an invoked command, as `(event, format)`. Only user actions are
/// listed: listings, thumbnails and other background calls would drown them out.
/// Nothing from the arguments is sent, so no path or file name leaves the machine.
fn command_event(command: &str) -> Option<(&'static str, Option<&str>)> {
    if let Some(format) = command
        .strip_prefix("open_")
        .filter(|format| PREVIEW_FORMATS.contains(format))
    {
        return Some(("file_previewed", Some(format)));
    }
    let event = match command {
        "open_viewer" => "viewer_opened",
        "open_path" => "file_opened",
        "open_with" => "file_opened_with",
        "reveal_path" => "file_revealed",
        "open_terminal" => "terminal_opened",
        "open_settings" => "settings_opened",
        "search_directory" => "search_performed",
        "create_item" => "item_created",
        "rename_item" => "item_renamed",
        "copy_item" | "copy_items" => "items_copied",
        "move_item" | "move_items" => "items_moved",
        "trash_item" => "items_trashed",
        "delete_item" | "delete_items" => "items_deleted",
        "create_archive" => "archive_created",
        "extract_archive" => "archive_extracted",
        "add_favorite" => "favorite_added",
        "remove_file_exif" => "exif_removed",
        "empty_trash" => "trash_emptied",
        "eject_volume" => "volume_ejected",
        "add_remote_location" => "remote_location_added",
        "connect_google_drive" => "google_drive_connected",
        "remove_remote_location" => "remote_location_removed",
        "upload_remote_files" => "remote_uploaded",
        "download_remote_file" | "download_remote_items" => "remote_downloaded",
        "delete_remote_items" => "remote_deleted",
        "create_remote_bucket" => "bucket_created",
        "add_network_location" => "network_location_added",
        "connect_network_location" => "network_connected",
        "mount_network_share" => "network_share_mounted",
        "scan_smb_servers" => "network_scan_started",
        _ => return None,
    };
    Some((event, None))
}

/// `open_<format>` preview commands, reported as `file_previewed` with that format.
const PREVIEW_FORMATS: &[&str] = &[
    "epub",
    "rtf",
    "word",
    "presentation",
    "mail",
    "mbox",
    "vcards",
    "calendar",
    "torrent",
    "data",
    "comic",
    "notebook",
    "database",
    "duckdb",
    "subtitle",
    "certificate",
    "geo",
    "fb2",
    "pcap",
    "iso",
    "msg",
    "sketch",
    "psd",
    "dicom",
    "mobi",
    "avro",
    "parquet",
    "arrow",
];

/// Wraps the app's command handler so listed commands also send a usage event. Every
/// command still runs exactly as before; tracking only reads its name.
pub fn with_usage_events(
    handler: impl Fn(Invoke) -> bool + Send + Sync + 'static,
) -> impl Fn(Invoke) -> bool + Send + Sync + 'static {
    move |invoke| {
        if let Some((event, format)) = command_event(invoke.message.command()) {
            let app = invoke.message.webview_ref().app_handle();
            if let Some(analytics) = app.try_state::<Analytics>() {
                let props = format.map(|format| serde_json::json!({ "format": format }));
                analytics.track(app, event, props);
            }
        }
        handler(invoke)
    }
}

/// Usage event from the frontend, for actions that never reach a Rust command.
/// Properties that are not strings or numbers are dropped, as Aptabase rejects them.
#[tauri::command]
pub fn track_event(
    app: AppHandle,
    analytics: State<'_, Analytics>,
    name: String,
    props: Option<Map<String, Value>>,
) {
    let props = props.map(|props| {
        Value::Object(
            props
                .into_iter()
                .filter(|(_, value)| value.is_string() || value.is_number())
                .collect(),
        )
    });
    analytics.track(&app, &name, props);
}

/// The toggle is locked on until 1.0.
fn is_locked(app: &AppHandle) -> bool {
    app.package_info().version.major < 1
}

#[tauri::command]
pub fn analytics_prefs(analytics: State<'_, Analytics>) -> AnalyticsPrefs {
    analytics.prefs()
}

/// Persists the preference and flips the live gate. Before 1.0 `enabled` is forced on.
#[tauri::command]
pub fn save_analytics(
    app: AppHandle,
    analytics: State<'_, Analytics>,
    enabled: bool,
    welcome_seen: bool,
) -> AnalyticsPrefs {
    let enabled = if is_locked(&app) { true } else { enabled };
    let was_enabled = analytics.enabled();
    analytics.store(AnalyticsPrefs {
        enabled,
        welcome_seen,
        install_id: analytics.install_id(),
    });
    // Sessions ignore `before_send`, so opting out has to end the session itself.
    if was_enabled && !enabled {
        sentry::end_session();
    } else if !was_enabled && enabled {
        sentry::start_session();
    }
    analytics.prefs()
}
