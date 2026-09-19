//! Shared progress reporting and cancellation for long-running transfers.
//!
//! S3 transfers and LAN (LocalSend) transfers emit the same `transfer-progress`
//! event so the frontend has a single Activity model. Every transfer registers a
//! `CancellationToken` under its job id; `cancel_transfer` fires it.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

pub const TRANSFER_EVENT: &str = "transfer-progress";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProgress {
    pub path: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// One progress snapshot for a transfer job.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TransferEvent {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub destination: String,
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub state: String,
    pub error: Option<String>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_progress: Option<FileProgress>,
    /// Set once when a whole queued item (top-level source) finishes, so the
    /// clipboard can drop it from its list without waiting for the whole job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

impl TransferEvent {
    pub fn start_download(&mut self, path: &std::path::Path, bytes_total: u64) {
        self.file_progress = Some(FileProgress {
            path: path.to_string_lossy().into_owned(),
            bytes_done: 0,
            bytes_total,
        });
    }

    pub fn add_bytes(&mut self, bytes: u64) {
        self.bytes_done += bytes;
        if let Some(file) = &mut self.file_progress {
            file.bytes_done += bytes;
        }
    }

    pub fn new(
        id: String,
        kind: &str,
        destination: String,
        files_total: u64,
        bytes_total: u64,
    ) -> Self {
        Self {
            id,
            kind: kind.to_string(),
            label: String::new(),
            destination,
            files_total,
            files_done: 0,
            bytes_total,
            bytes_done: 0,
            state: "active".to_string(),
            error: None,
            started_at: timestamp_ms(),
            finished_at: None,
            file_progress: None,
            item: None,
        }
    }
}

pub fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn emit(app: &AppHandle, event: &TransferEvent) {
    let mut snapshot = event.clone();
    if matches!(snapshot.state.as_str(), "done" | "failed" | "cancelled") {
        snapshot.finished_at = Some(timestamp_ms());
        snapshot.file_progress = None;
    }
    let _ = app.emit(TRANSFER_EVENT, snapshot);
}

/// Job id to cancellation token.
#[derive(Default)]
pub struct TransferRegistry(Mutex<HashMap<String, CancellationToken>>);

impl TransferRegistry {
    pub fn register(&self, id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        self.0.lock().unwrap().insert(id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, id: &str) {
        if let Some(token) = self.0.lock().unwrap().get(id) {
            token.cancel();
        }
    }

    pub fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

#[tauri::command]
pub fn cancel_transfer(registry: tauri::State<'_, TransferRegistry>, id: String) {
    registry.cancel(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_progress_uses_destination_and_resets_between_files() {
        let mut event = TransferEvent::new("job".into(), "download", "/downloads".into(), 2, 300);
        event.start_download(std::path::Path::new("/downloads/file (1).zip"), 100);
        event.add_bytes(100);
        assert_eq!(
            event.file_progress.as_ref().unwrap().path,
            "/downloads/file (1).zip"
        );
        assert_eq!(event.file_progress.as_ref().unwrap().bytes_done, 100);
        event.start_download(std::path::Path::new("/downloads/nested/second.zip"), 200);
        event.add_bytes(50);
        let file = event.file_progress.as_ref().unwrap();
        assert_eq!((file.bytes_done, file.bytes_total), (50, 200));
        assert_eq!(event.bytes_done, 150);
        event.file_progress = None;
        assert!(serde_json::to_value(&event)
            .unwrap()
            .get("fileProgress")
            .is_none());
    }
}
