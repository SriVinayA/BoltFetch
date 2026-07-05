use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub chunk_id: usize,
    pub thread_id: usize,
    pub start: u64,
    pub current: u64,
    pub end: u64,
    pub thread_downloaded: u64,
    pub status: String,
}

pub trait EventEmitter: Send + Sync {
    fn emit_filename_resolved(&self, filename: String);
    fn emit_progress(&self, payload: ProgressPayload);
}

pub struct TauriEventEmitter {
    app: AppHandle,
}

impl TauriEventEmitter {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventEmitter for TauriEventEmitter {
    fn emit_filename_resolved(&self, filename: String) {
        let _ = self.app.emit("filename-resolved", filename);
    }

    fn emit_progress(&self, payload: ProgressPayload) {
        let _ = self.app.emit("download-progress", payload);
    }
}
