//! Apple Intelligence name suggestions for the rename field.
//!
//! Only Macs with Apple silicon on macOS 26 or later can run the on-device model. Everywhere else
//! both commands report "unavailable" with a reason, so the frontend never has to guess: it asks
//! once, hides the affordance when the answer is no, and shows the reason when the user finds the
//! menu item anyway.
//!
//! The bundled Swift dylib is linked *weakly* (see `build.rs`), which is what keeps macOS 15
//! launchable — and it is also why every call goes through [`os_supports_model`]: on an older
//! system the dylib resolves to nothing, and calling into it would crash instead of failing.

use serde::Serialize;
use tauri::AppHandle;

mod naming;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::Path;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use tauri_plugin_apple_intelligence::{
    AppleAIGenerateRequest, AppleAIGenerateResult, AppleAIMessage, AppleIntelligenceExt,
};

/// First macOS with the Foundation Models framework.
const MIN_MAJOR: i32 = 26;
/// Availability turns on and off with the system setting, but a slow or failing check should not
/// run on every keystroke either, so the answer is reused for a minute.
const STATUS_TTL: std::time::Duration = std::time::Duration::from_secs(60);

const UNAVAILABLE: &str =
    "Apple Intelligence needs a Mac with Apple silicon running macOS 26 or later.";

/// Whether the model can be used right now, and why not when it can't.
#[derive(Clone, Serialize)]
pub struct AppleIntelligenceStatus {
    pub available: bool,
    pub reason: String,
}

/// Asks for a better name for the item at `path`.
#[tauri::command]
pub async fn suggest_name(app: AppHandle, path: String) -> Result<String, String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        suggest_with_model(&app, &path).await
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = (&app, &path);
        Err(UNAVAILABLE.to_string())
    }
}

/// Reports whether name suggestions can be offered at all.
#[tauri::command]
pub async fn apple_intelligence_status(app: AppHandle) -> AppleIntelligenceStatus {
    status(&app).await
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
async fn suggest_with_model(app: &AppHandle, path: &str) -> Result<String, String> {
    let status = status(app).await;
    if !status.available {
        return Err(status.reason);
    }
    let app = app.clone();
    let path = path.to_string();
    // Generation blocks on the model for seconds; keep it off the async runtime.
    tauri::async_runtime::spawn_blocking(move || {
        let context = naming::context_for(Path::new(&path))?;
        let original_extension = context.extension.clone();
        let request = AppleAIGenerateRequest {
            messages: vec![
                AppleAIMessage {
                    role: "system".into(),
                    content: Some(naming::system_prompt(
                        context.is_folder,
                        context.extension.as_deref(),
                    )),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    images: None,
                },
                AppleAIMessage {
                    role: "user".into(),
                    content: Some(naming::user_prompt(&context)),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    images: None,
                },
            ],
            tools: None,
            schema: Some(naming::schema()),
            model: Some("on-device".into()),
            reasoning_level: None,
            temperature: Some(0.3),
            max_tokens: Some(64),
            top_p: None,
            top_k: None,
            seed: None,
            tool_choice: None,
            stop_after_tool_calls: None,
        };
        let result = app
            .apple_intelligence()
            .generate(request)
            .map_err(|error| error.to_string())?;
        name_from(&result, &original_extension)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Prefers the guided `name` field and falls back to the first line of free text.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn name_from(
    result: &AppleAIGenerateResult,
    original_extension: &Option<String>,
) -> Result<String, String> {
    let raw = result
        .object
        .as_ref()
        .and_then(|object| object.get("name"))
        .and_then(|name| name.as_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            let text = result.text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        })
        .ok_or_else(|| "Apple Intelligence didn't suggest a name.".to_string())?;
    naming::sanitize_name(&raw, original_extension.as_deref())
        .ok_or_else(|| "Apple Intelligence suggested a name this folder can't use.".to_string())
}

/// Cached availability, so opening the rename field stays instant.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
async fn status(app: &AppHandle) -> AppleIntelligenceStatus {
    if let Some(status) = cached_status() {
        return status;
    }
    let app = app.clone();
    let status = tauri::async_runtime::spawn_blocking(move || detect(&app))
        .await
        .unwrap_or_else(|error| AppleIntelligenceStatus {
            available: false,
            reason: error.to_string(),
        });
    if let Ok(mut cache) = STATUS_CACHE.lock() {
        *cache = Some((std::time::Instant::now(), status.clone()));
    }
    status
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
async fn status(_app: &AppHandle) -> AppleIntelligenceStatus {
    AppleIntelligenceStatus {
        available: false,
        reason: UNAVAILABLE.to_string(),
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn detect(app: &AppHandle) -> AppleIntelligenceStatus {
    if !os_supports_model() {
        return AppleIntelligenceStatus {
            available: false,
            reason: UNAVAILABLE.to_string(),
        };
    }
    match app.apple_intelligence().check_availability() {
        Ok(availability) => AppleIntelligenceStatus {
            available: availability.available,
            reason: if availability.available {
                "Apple Intelligence is ready".to_string()
            } else {
                availability.reason
            },
        },
        Err(error) => AppleIntelligenceStatus {
            available: false,
            reason: error.to_string(),
        },
    }
}

/// `kern.osproductversion` read through sysctl — no subprocess, no plist parsing.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn os_supports_model() -> bool {
    let name = b"kern.osproductversion\0";
    let mut size: libc::size_t = 0;
    let probe = unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if probe != 0 || size == 0 {
        return false;
    }
    let mut buffer = vec![0u8; size];
    let read = unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if read != 0 {
        return false;
    }
    let text = std::str::from_utf8(&buffer).unwrap_or("");
    let version = text.trim_end_matches('\0');
    naming::parse_os_version(version).is_some_and(|(major, _)| major >= MIN_MAJOR)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
static STATUS_CACHE: std::sync::Mutex<Option<(std::time::Instant, AppleIntelligenceStatus)>> =
    std::sync::Mutex::new(None);

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn cached_status() -> Option<AppleIntelligenceStatus> {
    let cache = STATUS_CACHE.lock().ok()?;
    let (at, status) = cache.as_ref()?;
    (at.elapsed() < STATUS_TTL).then(|| status.clone())
}

#[cfg(test)]
mod tests {
    use super::{AppleIntelligenceStatus, UNAVAILABLE};
    use crate::ai::naming::sanitize_name;

    #[test]
    fn reasons_are_never_empty() {
        let status = AppleIntelligenceStatus {
            available: false,
            reason: UNAVAILABLE.to_string(),
        };
        assert!(!status.reason.is_empty());
    }

    #[test]
    fn suggested_names_are_cleaned_up() {
        assert_eq!(
            sanitize_name("  Report.pdf ", Some("pdf")).as_deref(),
            Some("Report.pdf")
        );
    }
}
