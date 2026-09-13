//! Shared progress reporting and cancellation for long-running transfers.
//!
//! S3 transfers and LAN (LocalSend) transfers emit the same `transfer-progress`
//! event so the frontend has a single Activity model. Every transfer registers a
//! `CancellationToken` under its job id; `cancel_transfer` fires it.

use std::{collections::HashMap, sync::Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

pub const TRANSFER_EVENT: &str = "transfer-progress";

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
}

impl TransferEvent {
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
        }
    }
}

pub fn emit(app: &AppHandle, event: &TransferEvent) {
    let _ = app.emit(TRANSFER_EVENT, event.clone());
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
