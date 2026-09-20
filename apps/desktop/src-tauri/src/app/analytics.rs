//! Anonymous error reporting (Sentry) and its opt-out preference.
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
use tauri::{AppHandle, State};
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
        enabled.then_some(event)
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
pub fn capture_first_run(welcome_seen: bool) {    if welcome_seen {
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
