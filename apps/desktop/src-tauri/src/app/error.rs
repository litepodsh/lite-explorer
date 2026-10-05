use serde::{Deserialize, Serialize};

/// Structured error sent to the frontend. Serialized as
/// `{ "kind": "notFound", "path": "/x", ... }` (camelCase fields), matching
/// the existing `ConnectError` shape so `describeError` can handle both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AppError {
    #[error("Access {path}: {message}")]
    #[serde(rename_all = "camelCase")]
    Io { path: String, message: String },
    #[error("{path} no longer exists")]
    NotFound { path: String },
    #[error("No permission to access {path}")]
    PermissionDenied { path: String },
    #[error("Invalid path: {path}")]
    InvalidPath { path: String },
    #[error("Timed out: {operation}")]
    Timeout { operation: String },
    #[error("{message}")]
    Unreachable { message: String },
    #[error("Cancelled")]
    Cancelled,
    #[error("Completed {completed} of {total} items")]
    #[serde(rename_all = "camelCase")]
    Partial {
        completed: usize,
        total: usize,
        failed: Vec<FailedItem>,
    },
    #[error("Unsupported: {feature}")]
    Unsupported { feature: String },
    #[error("{message}")]
    Panic { message: String },
    #[error("{message}")]
    Other { message: String },
}

/// One failed item inside a batch operation (`AppError::Partial`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedItem {
    pub path: String,
    pub error: String,
}

impl AppError {
    /// Wraps an `io::Error` with its path, mapping the kind to a structured
    /// variant so the frontend can show the right copy and the retry layer
    /// (Fase 4) can act on it.
    #[allow(dead_code)] // used by Fase 2 (atomicidad) migrations
    pub fn io(path: impl Into<String>, error: std::io::Error) -> Self {
        let path = path.into();
        match error.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound { path },
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied { path },
            other => Self::Io {
                path,
                message: match other {
                    std::io::ErrorKind::TimedOut => "The operation timed out".into(),
                    _ => error.to_string(),
                },
            },
        }
    }

    #[allow(dead_code)]
    pub fn not_found(path: impl Into<String>) -> Self {
        Self::NotFound { path: path.into() }
    }

    #[allow(dead_code)]
    pub fn invalid_path(path: impl Into<String>) -> Self {
        Self::InvalidPath { path: path.into() }
    }

    #[allow(dead_code)]
    pub fn unreachable(message: impl Into<String>) -> Self {
        Self::Unreachable {
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self::Cancelled
    }

    pub fn panic(message: impl Into<String>) -> Self {
        Self::Panic {
            message: message.into(),
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other {
            message: message.into(),
        }
    }

    pub fn partial(completed: usize, total: usize, failed: Vec<FailedItem>) -> Self {
        Self::Partial {
            completed,
            total,
            failed,
        }
    }

    /// Human message for logs (no path context needed).
    #[allow(dead_code)] // used by Fase 3 (network timeouts)
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        AppError::other(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        AppError::other(message)
    }
}

impl From<crate::network::ConnectError> for AppError {
    fn from(error: crate::network::ConnectError) -> Self {
        use crate::network::ErrorKind;
        match error.kind {
            ErrorKind::NotFound => AppError::NotFound {
                path: error.message,
            },
            ErrorKind::Timeout => AppError::Timeout {
                operation: error.message,
            },
            ErrorKind::Unsupported => AppError::Unsupported {
                feature: error.message,
            },
            _ => AppError::Unreachable {
                message: error.message,
            },
        }
    }
}

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Runs a blocking closure (used inside `spawn_blocking`), converting any
/// panic into `AppError::Panic` instead of letting it kill the process.
pub fn guarded<T>(body: impl FnOnce() -> Result<T, AppError>) -> Result<T, AppError> {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(result) => result,
        Err(payload) => Err(AppError::panic(panic_message(payload))),
    }
}

/// Converts a `spawn_blocking` handle error into an `AppError`, extracting the
/// panic message when the blocking task panicked (instead of a generic join error).
pub fn from_join(error: tauri::Error) -> AppError {
    match error {
        tauri::Error::JoinError(join) if join.is_panic() => {
            AppError::panic(panic_message(join.into_panic()))
        }
        tauri::Error::JoinError(join) => AppError::other(join.to_string()),
        other => AppError::other(other.to_string()),
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "Unexpected panic".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_not_found_serializes_kind_and_path() {
        let json = serde_json::to_value(AppError::NotFound { path: "/x".into() }).unwrap();
        assert_eq!(json["kind"], "notFound");
        assert_eq!(json["path"], "/x");
    }

    #[test]
    fn io_not_found_maps_to_not_found_variant() {
        let error = AppError::io("/x", std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(error, AppError::NotFound { path: "/x".into() });
    }

    #[test]
    fn io_permission_denied_maps_to_permission_variant() {
        let error = AppError::io(
            "/x",
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        assert_eq!(error, AppError::PermissionDenied { path: "/x".into() });
    }

    #[test]
    fn io_other_kind_maps_to_io_with_path() {
        let inner = std::io::Error::other("disk melted");
        let error = AppError::io("/x", inner);
        assert!(matches!(
            error,
            AppError::Io { path, message } if path == "/x" && message.contains("disk melted")
        ));
    }

    #[test]
    fn from_string_produces_other() {
        let error: AppError = "boom".into();
        assert_eq!(
            error,
            AppError::Other {
                message: "boom".into()
            }
        );
    }

    #[test]
    fn guarded_converts_panic_to_app_error() {
        let result = guarded(|| -> Result<(), AppError> { panic!("boom") });
        assert!(matches!(
            result,
            Err(AppError::Panic { message }) if message == "boom"
        ));
    }

    #[test]
    fn guarded_returns_ok_when_no_panic() {
        let result = guarded(|| Ok::<_, AppError>(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn from_join_extracts_panic_message() {
        let error: tauri::Error = tauri::async_runtime::block_on(async {
            tauri::async_runtime::spawn_blocking(|| -> i32 { panic!("blocking boom") })
                .await
                .unwrap_err()
        });
        // Wait: spawn_blocking JoinError → tauri::Error::JoinError.
        let app_error = from_join(error);
        assert!(matches!(
            app_error,
            AppError::Panic { message } if message.contains("blocking boom")
        ));
    }
}
