use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::path::PathBuf;
use futures::future::join_all;

use crate::events::{ProgressEmitter, ProgressPayload};
use crate::state::{ChunkState, StateManager, DownloadState};

use tokio::sync::oneshot;
use std::sync::atomic::AtomicU64;
use std::collections::HashMap;

#[allow(dead_code)]
pub enum StateMessage {
    RequestWork {
        reply: oneshot::Sender<Option<(ChunkState, Arc<AtomicU64>)>>,
    },
    UpdateProgress {
        chunk_id: usize,
        current: u64,
    },
    ChunkComplete {
        chunk_id: usize,
    },
    SaveStateNow,
}

#[allow(dead_code)]
pub struct StateManagerTask {
    pub download_state: DownloadState,
    pub active_chunks: HashMap<usize, Arc<AtomicU64>>,
    pub state_file_path: PathBuf,
}

#[allow(dead_code)]
impl StateManagerTask {
    fn get_work(&mut self) -> Option<(ChunkState, Arc<AtomicU64>)> {
        // Find a pending chunk that isn't active
        if let Some(c) = self.download_state.chunks.iter().find(|c| !self.active_chunks.contains_key(&c.id) && c.current <= c.end) {
            let end_atomic = Arc::new(AtomicU64::new(c.end));
            self.active_chunks.insert(c.id, end_atomic.clone());
            return Some((c.clone(), end_atomic));
        }

        let min_chunk_size = 1024 * 1024; // 1 MB
        let mut largest_idx = None;
        let mut max_remaining = 0;

        for (i, c) in self.download_state.chunks.iter().enumerate() {
            if c.current <= c.end {
                let remaining = c.end - c.current;
                if remaining > max_remaining && remaining >= min_chunk_size * 2 {
                    max_remaining = remaining;
                    largest_idx = Some(i);
                }
            }
        }

        if let Some(idx) = largest_idx {
            let next_id = self.download_state.chunks.iter().map(|c| c.id).max().unwrap_or(0) + 1;
            let c = &mut self.download_state.chunks[idx];
            let remaining = c.end - c.current;
            let half = remaining / 2;
            
            let new_c = ChunkState {
                id: next_id,
                start: c.end - half + 1,
                current: c.end - half + 1,
                end: c.end,
            };
            c.end = c.end - half;
            
            // Atomically update the old chunk's end bound for the existing worker
            if let Some(atomic_end) = self.active_chunks.get(&c.id) {
                atomic_end.store(c.end, Ordering::Relaxed);
            }
            
            let new_end_atomic = Arc::new(AtomicU64::new(new_c.end));
            self.active_chunks.insert(new_c.id, new_end_atomic.clone());
            self.download_state.chunks.push(new_c.clone());
            return Some((new_c, new_end_atomic));
        }

        None
    }
}



pub struct Downloader {
    client: Client,
    progress: Arc<dyn ProgressEmitter>,
}

impl Downloader {
    pub fn new(progress: Arc<dyn ProgressEmitter>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .tcp_nodelay(true)
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        Ok(Self { client, progress })
    }

    pub async fn download(
        &self,
        url: &str,
        output_dir: Option<PathBuf>,
        fallback_filename: &str,
        mut num_threads: u64,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<String, String> {
        let range_check = self.client.get(url).header(RANGE, "bytes=0-0").send().await.map_err(|e| format!("Network error: {}", e))?;
        let supports_range = range_check.status() == StatusCode::PARTIAL_CONTENT;
        
        if !supports_range && range_check.status() != StatusCode::OK {
            return Err(format!("Server returned HTTP {}", range_check.status()));
        }

        let mut final_filename = fallback_filename.to_string();
        if let Some(cd_header) = range_check.headers().get(CONTENT_DISPOSITION) {
            if let Ok(cd_str) = cd_header.to_str() {
                if let Some(name_start) = cd_str.find("filename=") {
                    let rest = &cd_str[name_start + 9..];
                    let name = rest.split(';').next().unwrap_or(rest).trim_matches(|c| c == '"' || c == '\'');
                    if !name.is_empty() { final_filename = name.to_string(); }
                }
            }
        }
        if final_filename == fallback_filename {
            if let Some(path_segment) = range_check.url().path_segments().and_then(|s| s.last()) {
                if !path_segment.is_empty() && path_segment.contains('.') {
                    final_filename = path_segment.split('?').next().unwrap_or(path_segment).to_string();
                }
            }
        }
        
        self.progress.emit_filename_resolved(final_filename.clone());

        if !supports_range {
            self.progress.emit_log("Server does not support multipart downloads. Falling back to single thread.".to_string());
            num_threads = 1;
        }

        let content_length: u64 = if supports_range {
            range_check.headers().get(CONTENT_RANGE)
                .and_then(|v| v.to_str().ok()).ok_or("No Content-Range header")?
                .split('/').last().and_then(|s| s.parse().ok()).unwrap_or(0)
        } else {
            range_check.headers().get(reqwest::header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse().ok()).unwrap_or(0)
        };

        let mut file_path = output_dir.unwrap_or_else(|| PathBuf::from("."));
        file_path.push(&final_filename);
        let state_file_path = file_path.with_extension("boltfetch");

        let (download_state, is_resume) = StateManager::load_or_init(&state_file_path, url, content_length, supports_range);

        if !is_resume {
            let file = File::create(&file_path).map_err(|e| format!("Failed to create file: {}", e))?;
            file.set_len(content_length).map_err(|e| format!("Failed to allocate disk space: {}", e))?;
        } else if !file_path.exists() {
            return Err("Target file is missing. Please delete the .boltfetch file and start over.".into());
        }

        for chunk in &download_state.chunks {
            self.progress.emit_progress(ProgressPayload {
                chunk_id: chunk.id,
                thread_id: chunk.id,
                start: chunk.start,
                current: chunk.current,
                end: chunk.end,
                total_size: content_length,
                thread_downloaded: chunk.current.saturating_sub(chunk.start),
                status: "Initializing...".to_string(),
            });
        }

        let (state_tx, mut state_rx) = tokio::sync::mpsc::channel::<StateMessage>(100);
        
        let mut sm_task = StateManagerTask {
            download_state,
            active_chunks: HashMap::new(),
            state_file_path: state_file_path.clone(),
        };
        
        tokio::spawn(async move {
            let mut last_save = std::time::Instant::now();
            let save_interval = std::time::Duration::from_millis(500);

            while let Some(msg) = state_rx.recv().await {
                let mut needs_save = false;
                
                match msg {
                    StateMessage::RequestWork { reply } => {
                        if let Err(_) = reply.send(sm_task.get_work()) {
                            eprintln!("Error: failed to send RequestWork reply");
                        }
                        needs_save = true;
                    }
                    StateMessage::UpdateProgress { chunk_id, current } => {
                        if let Some(c) = sm_task.download_state.chunks.iter_mut().find(|c| c.id == chunk_id) {
                            c.current = current;
                        }
                    }
                    StateMessage::ChunkComplete { chunk_id } => {
                        sm_task.active_chunks.remove(&chunk_id);
                        needs_save = true;
                    }
                    StateMessage::SaveStateNow => {
                        needs_save = true;
                    }
                }

                if needs_save || last_save.elapsed() >= save_interval {
                    if let Err(e) = StateManager::save(&sm_task.state_file_path, &sm_task.download_state) {
                        eprintln!("Error: failed to save state: {}", e);
                    }
                    last_save = std::time::Instant::now();
                }
            }
            // Final save on shutdown
            if let Err(e) = StateManager::save(&sm_task.state_file_path, &sm_task.download_state) {
                eprintln!("Error: failed to save state on shutdown: {}", e);
            }
        });

        let file = std::fs::OpenOptions::new().write(true).open(&file_path).map_err(|e| format!("Failed to open file: {}", e))?;
        let file_arc = Arc::new(file);

        let mut tasks = Vec::new();
        
        let generic_error_flag = Arc::new(tokio::sync::Mutex::new(String::new()));
        let rate_limit_hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        for thread_id in 0..num_threads {
            let client_clone = self.client.clone();
            let file_clone = file_arc.clone();
            let url_clone = url.to_string();
            let progress_clone = self.progress.clone();
            let state_tx_clone = state_tx.clone();
            let cancel_flag_clone = cancel_flag.clone();
            let err_flag = generic_error_flag.clone();
            let rl_hits = rate_limit_hits.clone();
            let thread_content_length = content_length;

            let task = tokio::spawn(async move {
                loop {
                    if cancel_flag_clone.load(Ordering::Relaxed) {
                        *err_flag.lock().await = "Paused by user".to_string();
                        break;
                    }

                    let (chunk, atomic_end) = {
                        let (reply_tx, reply_rx) = oneshot::channel();
                        if state_tx_clone.send(StateMessage::RequestWork { reply: reply_tx }).await.is_err() {
                            break;
                        }
                        match reply_rx.await.unwrap_or(None) {
                            Some(work) => work,
                            None => break, // No more work
                        }
                    };
                    
                    let mut retries = 0;
                    let max_retries = 5;

                    let mut current_offset = chunk.current;
                    let mut downloaded_for_this_thread = chunk.current.saturating_sub(chunk.start);

                    let response = loop {
                        let range_header = format!("bytes={}-{}", current_offset, atomic_end.load(Ordering::Relaxed));
                        match client_clone.get(&url_clone).header(RANGE, range_header).send().await {
                            Ok(res) => {
                                if res.status() == StatusCode::TOO_MANY_REQUESTS || res.status() == StatusCode::SERVICE_UNAVAILABLE {
                                    if retries >= max_retries {
                                        rl_hits.fetch_add(1, Ordering::SeqCst);
                                        let _ = state_tx_clone.send(StateMessage::ChunkComplete { chunk_id: chunk.id }).await;
                                        return; // Give up
                                    }
                                    retries += 1;
                                    tokio::time::sleep(tokio::time::Duration::from_secs(1 << retries)).await;
                                    continue;
                                } else if !res.status().is_success() {
                                    *err_flag.lock().await = format!("HTTP {}", res.status());
                                    let _ = state_tx_clone.send(StateMessage::ChunkComplete { chunk_id: chunk.id }).await;
                                    return;
                                }
                                break res;
                            },
                            Err(e) => {
                                if retries >= max_retries {
                                    *err_flag.lock().await = format!("Network error: {}", e);
                                    let _ = state_tx_clone.send(StateMessage::ChunkComplete { chunk_id: chunk.id }).await;
                                    return;
                                }
                                retries += 1;
                                tokio::time::sleep(tokio::time::Duration::from_secs(1 << retries)).await;
                            }
                        }
                    };

                    let mut chunk_stream = response;
                    while let Some(chunk_bytes) = chunk_stream.chunk().await.unwrap_or(None) {
                        if cancel_flag_clone.load(Ordering::Relaxed) { 
                            *err_flag.lock().await = "Paused by user".to_string();
                            break; 
                        }

                        let f = file_clone.clone();
                        let offset = current_offset;
                        
                        let write_result = tokio::task::spawn_blocking(move || -> std::io::Result<u64> {
                            let mut chunk_offset = 0;
                            while chunk_offset < chunk_bytes.len() {
                                let written = f.write_at(&chunk_bytes[chunk_offset..], offset + chunk_offset as u64)?;
                                if written == 0 {
                                    return Err(std::io::Error::new(std::io::ErrorKind::WriteZero, "Failed to write whole buffer"));
                                }
                                chunk_offset += written;
                            }
                            Ok(chunk_offset as u64)
                        }).await;

                        let bytes_written = match write_result {
                            Ok(Ok(bw)) => bw,
                            Ok(Err(e)) => {
                                *err_flag.lock().await = format!("Disk I/O error: {}", e);
                                break;
                            },
                            Err(e) => {
                                *err_flag.lock().await = format!("Task join error: {}", e);
                                break;
                            }
                        };

                        current_offset += bytes_written;
                        downloaded_for_this_thread += bytes_written;

                        let _ = state_tx_clone.send(StateMessage::UpdateProgress { 
                            chunk_id: chunk.id, 
                            current: current_offset 
                        }).await;

                        progress_clone.emit_progress(ProgressPayload {
                            chunk_id: chunk.id,
                            thread_id: thread_id as usize,
                            start: chunk.start,
                            current: current_offset,
                            end: atomic_end.load(Ordering::Relaxed),
                            total_size: thread_content_length,
                            thread_downloaded: downloaded_for_this_thread,
                            status: "Receiving data...".to_string(),
                        });
                        
                        if current_offset >= atomic_end.load(Ordering::Relaxed) {
                            break;
                        }
                    }

                    let _ = state_tx_clone.send(StateMessage::ChunkComplete { chunk_id: chunk.id }).await;
                    
                    let has_err = err_flag.lock().await.clone();
                    if !has_err.is_empty() {
                        break;
                    }
                }
            });

            tasks.push(task);
        }

        drop(state_tx);
        join_all(tasks).await;

        let err = generic_error_flag.lock().await.clone();
        let hits = rate_limit_hits.load(Ordering::SeqCst);
        
        if hits > 0 {
            return Err(format!("RATE_LIMIT:{}:{}", num_threads as usize - hits, num_threads));
        }

        if !err.is_empty() {
            if err == "Paused by user" {
                return Err("Download Paused. Ready to Resume.".to_string());
            }
            return Err(err);
        }

        let _ = std::fs::remove_file(&state_file_path);
        Ok(format!("Saved to {}", file_path.display()))
    }
}
