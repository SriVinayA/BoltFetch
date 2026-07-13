use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkState {
    pub id: usize,
    pub start: u64,
    pub current: u64,
    pub end: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DownloadState {
    pub url: String,
    pub total_size: u64,
    pub chunks: Vec<ChunkState>,
}

pub struct StateManager;

impl StateManager {
    pub fn load_or_init(
        state_file_path: &Path,
        url: &str,
        content_length: u64,
        supports_range: bool,
    ) -> (DownloadState, bool) {
        let mut download_state = DownloadState {
            url: url.to_string(),
            total_size: content_length,
            chunks: Vec::new(),
        };
        let mut is_resume = false;

        if supports_range && state_file_path.exists() {
            if let Ok(content) = std::fs::read_to_string(state_file_path) {
                if let Ok(parsed) = serde_json::from_str::<DownloadState>(&content) {
                    if parsed.url == url && parsed.total_size == content_length {
                        download_state = parsed;
                        is_resume = true;
                    }
                }
            }
        }

        if !is_resume {
            download_state.chunks.push(ChunkState {
                id: 0,
                start: 0,
                current: 0,
                end: if content_length > 0 { content_length - 1 } else { 0 },
            });
        }

        (download_state, is_resume)
    }

    pub fn save(state_file_path: &Path, state: &DownloadState) -> Result<(), std::io::Error> {
        let content = serde_json::to_string(state)?;
        std::fs::write(state_file_path, content)
    }
}
