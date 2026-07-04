use crate::events::{EventEmitter, ProgressPayload};
use crate::state::{ChunkState, StateManager};
use futures::stream::{self, StreamExt};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Downloader {
    client: Client,
    event_emitter: Arc<dyn EventEmitter>,
}

impl Downloader {
    pub fn new(event_emitter: Arc<dyn EventEmitter>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0")
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self {
            client,
            event_emitter,
        })
    }

    pub async fn download(
        &self,
        url: String,
        output: String,
        threads: u64,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        let range_check = self.client.get(&url).header(RANGE, "bytes=0-0").send().await.map_err(|e| format!("Network error: {}", e))?;
        if range_check.status() != StatusCode::PARTIAL_CONTENT {
            return Err("Server does not support multipart downloads. Cannot resume.".into());
        }

        let mut final_filename = output.clone();
        if let Some(cd_header) = range_check.headers().get(CONTENT_DISPOSITION) {
            if let Ok(cd_str) = cd_header.to_str() {
                if let Some(name_start) = cd_str.find("filename=") {
                    let rest = &cd_str[name_start + 9..];
                    let name = rest.split(';').next().unwrap_or(rest).trim_matches(|c| c == '"' || c == '\'');
                    if !name.is_empty() { final_filename = name.to_string(); }
                }
            }
        }
        if final_filename == output {
            if let Some(path_segment) = range_check.url().path_segments().and_then(|s| s.last()) {
                if !path_segment.is_empty() && path_segment.contains('.') {
                    final_filename = path_segment.split('?').next().unwrap_or(path_segment).to_string();
                }
            }
        }
        
        self.event_emitter.emit_filename_resolved(final_filename.clone());

        let content_length: u64 = range_check.headers().get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()).ok_or("No Content-Range header")?
            .split('/').last().and_then(|s| s.parse().ok()).ok_or("Failed to parse size")?;

        let mut file_path = dirs::download_dir().ok_or("Could not find Downloads directory")?;
        file_path.push(&final_filename);
        let state_file_path = file_path.with_extension("boltfetch");

        let (download_state, is_resume) = StateManager::load_or_init(&state_file_path, &url, content_length, threads);

        if !is_resume {
            let file = File::create(&file_path).map_err(|e| format!("Failed to create file: {}", e))?;
            file.set_len(content_length).map_err(|e| format!("Failed to allocate disk space: {}", e))?;
        } else if !file_path.exists() {
            return Err("Target file is missing. Please delete the .boltfetch file and start over.".into());
        }

        for chunk in &download_state.chunks {
            self.event_emitter.emit_progress(ProgressPayload {
                thread_id: chunk.id,
                chunk_size: chunk.end - chunk.start + 1,
                bytes_downloaded: chunk.current.saturating_sub(chunk.start),
            });
        }

        let shared_state = Arc::new(tokio::sync::Mutex::new(download_state.clone()));
        let pending_chunks: Vec<ChunkState> = download_state.chunks.into_iter().filter(|c| c.current <= c.end).collect();

        if pending_chunks.is_empty() {
            let _ = std::fs::remove_file(&state_file_path);
            return Ok(format!("Saved to {}", file_path.display()));
        }
        
        let total_pending = pending_chunks.len();
        let file = std::fs::OpenOptions::new().write(true).open(&file_path).map_err(|e| format!("Failed to open file: {}", e))?;
        let file = Arc::new(file);

        let pending_futures = pending_chunks.into_iter().map(|chunk| {
            let client_clone = self.client.clone();
            let file_clone = file.clone();
            let url_clone = url.clone();
            let event_emitter_clone = self.event_emitter.clone();
            let cancel_flag_clone = cancel_flag.clone();
            let shared_state_clone = shared_state.clone();
            let state_file_path_clone = state_file_path.clone();

            async move {
                let total_chunk_size = chunk.end - chunk.start + 1;
                let range_header = format!("bytes={}-{}", chunk.current, chunk.end);
                
                let mut response = client_clone.get(&url_clone).header(RANGE, range_header).send().await
                    .map_err(|e| format!("Chunk {} failed to connect: {}", chunk.id, e))?;

                if response.status() == StatusCode::TOO_MANY_REQUESTS || response.status() == StatusCode::SERVICE_UNAVAILABLE {
                    return Err("SERVER_BLOCKED".to_string());
                } else if !response.status().is_success() {
                    return Err(format!("Chunk {} failed: HTTP {}", chunk.id, response.status()));
                }

                let mut current_offset = chunk.current;
                let mut downloaded_for_this_thread = chunk.current - chunk.start;

                while let Some(chunk_bytes) = response.chunk().await.map_err(|e| format!("Chunk {} lost connection: {}", chunk.id, e))? {
                    if cancel_flag_clone.load(Ordering::Relaxed) { return Err("Paused by user".to_string()); }

                    let mut chunk_offset = 0;
                    while chunk_offset < chunk_bytes.len() {
                        let bytes_written = file_clone.write_at(&chunk_bytes[chunk_offset..], current_offset)
                            .map_err(|e| format!("Disk write error: {}", e))?;
                        chunk_offset += bytes_written;
                        current_offset += bytes_written as u64;
                        downloaded_for_this_thread += bytes_written as u64;
                    }

                    {
                        let mut s = shared_state_clone.lock().await;
                        if let Some(c) = s.chunks.iter_mut().find(|c| c.id == chunk.id) { c.current = current_offset; }
                        let _ = StateManager::save(&state_file_path_clone, &*s);
                    }

                    event_emitter_clone.emit_progress(ProgressPayload {
                        thread_id: chunk.id, chunk_size: total_chunk_size, bytes_downloaded: downloaded_for_this_thread,
                    });
                }
                Ok::<(), String>(())
            }
        });

        let stream = stream::iter(pending_futures).buffer_unordered(threads as usize);
        let results: Vec<Result<(), String>> = stream.collect().await;

        let mut rate_limit_hits = 0;
        let mut generic_error = String::new();

        for res in results {
            if let Err(e) = res {
                if e == "SERVER_BLOCKED" {
                    rate_limit_hits += 1;
                } else if generic_error.is_empty() {
                    generic_error = e;
                }
            }
        }

        if rate_limit_hits > 0 {
            let survived = total_pending - rate_limit_hits;
            return Err(format!("RATE_LIMIT:{}:{}", survived, threads));
        }

        if !generic_error.is_empty() {
            if generic_error == "Paused by user" { return Err("Download Paused. Ready to Resume.".to_string()); }
            return Err(generic_error);
        }

        let _ = std::fs::remove_file(&state_file_path);
        Ok(format!("Saved to {}", file_path.display()))
    }
}
