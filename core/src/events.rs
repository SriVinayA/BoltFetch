use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProgressPayload {
    pub chunk_id: usize,
    pub thread_id: usize,
    pub start: u64,
    pub current: u64,
    pub end: u64,
    pub total_size: u64,
    pub thread_downloaded: u64,
    pub status: String,
}

pub trait ProgressEmitter: Send + Sync {
    fn emit_progress(&self, payload: ProgressPayload);
    fn emit_filename_resolved(&self, filename: String);
    fn emit_log(&self, msg: String);
}
