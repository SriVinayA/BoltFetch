use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub thread_id: usize,
    pub chunk_size: u64,
    pub bytes_downloaded: u64,
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
