use crate::events::{EventEmitter, ProgressPayload};
use crate::state::{ChunkState, DownloadState, StateManager};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::collections::HashSet;

struct ActiveState {
    download_state: DownloadState,
    active_chunk_ids: HashSet<usize>,
    next_chunk_id: usize,
    rate_limit_hits: usize,
    global_error: Option<String>,
}

pub struct Downloader {
    client: Client,
    event_emitter: Arc<dyn EventEmitter>,
}

impl Downloader {
    pub fn new(event_emitter: Arc<dyn EventEmitter>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0")
            .tcp_nodelay(true)
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
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

        let max_id = download_state.chunks.iter().map(|c| c.id).max().unwrap_or(0);
        let active_state = Arc::new(tokio::sync::Mutex::new(ActiveState {
            download_state,
            active_chunk_ids: HashSet::new(),
            next_chunk_id: max_id + 1,
            rate_limit_hits: 0,
            global_error: None,
        }));

        let file = std::fs::OpenOptions::new().write(true).open(&file_path).map_err(|e| format!("Failed to open file: {}", e))?;
        let file = Arc::new(file);

        let mut tasks = Vec::new();
        for _ in 0..threads {
            let client_clone = self.client.clone();
            let file_clone = Arc::clone(&file);
            let url_clone = url.clone();
            let event_emitter_clone = self.event_emitter.clone();
            let cancel_flag_clone = Arc::clone(&cancel_flag);
            let shared_state_clone = Arc::clone(&active_state);
            let state_file_path_clone = state_file_path.clone();

            let task = tokio::spawn(async move {
                loop {
                    if cancel_flag_clone.load(Ordering::Relaxed) {
                        let mut s = shared_state_clone.lock().await;
                        s.global_error = Some("Paused by user".to_string());
                        break;
                    }

                    let chunk = {
                        let mut s = shared_state_clone.lock().await;
                        if s.global_error.is_some() || s.rate_limit_hits > 0 { break; }
                        
                        let mut next_chunk = None;
                        for c in &s.download_state.chunks {
                            if !s.active_chunk_ids.contains(&c.id) && c.current <= c.end {
                                next_chunk = Some(c.clone());
                                break;
                            }
                        }

                        if next_chunk.is_none() {
                            let mut largest_chunk_idx = None;
                            let mut max_remaining = 0;
                            
                            for (i, c) in s.download_state.chunks.iter().enumerate() {
                                let remaining = c.end.saturating_sub(c.current);
                                if remaining > 1024 * 1024 && remaining > max_remaining {
                                    max_remaining = remaining;
                                    largest_chunk_idx = Some(i);
                                }
                            }
                            
                            if let Some(idx) = largest_chunk_idx {
                                let old_chunk = s.download_state.chunks[idx].clone();
                                let remaining = old_chunk.end - old_chunk.current;
                                let split_point = old_chunk.current + (remaining / 2);
                                
                                s.download_state.chunks[idx].end = split_point - 1;
                                
                                let new_chunk = ChunkState {
                                    id: s.next_chunk_id,
                                    start: split_point,
                                    current: split_point,
                                    end: old_chunk.end,
                                };
                                s.next_chunk_id += 1;
                                s.download_state.chunks.push(new_chunk.clone());
                                
                                next_chunk = Some(new_chunk);
                            }
                        }

                        if let Some(c) = next_chunk {
                            s.active_chunk_ids.insert(c.id);
                            Some(c)
                        } else {
                            None
                        }
                    };

                    let chunk = match chunk {
                        Some(c) => c,
                        None => break, // No more work
                    };

                    let total_chunk_size = chunk.end - chunk.start + 1;
                    
                    if chunk.current > chunk.start {
                        event_emitter_clone.emit_progress(ProgressPayload {
                            thread_id: chunk.id,
                            chunk_size: total_chunk_size,
                            bytes_downloaded: chunk.current - chunk.start,
                        });
                    }

                    let range_header = format!("bytes={}-{}", chunk.current, chunk.end);
                    let mut response = match client_clone.get(&url_clone).header(RANGE, range_header).send().await {
                        Ok(res) => res,
                        Err(_) => {
                            let mut s = shared_state_clone.lock().await;
                            s.active_chunk_ids.remove(&chunk.id);
                            continue;
                        }
                    };

                    if response.status() == StatusCode::TOO_MANY_REQUESTS || response.status() == StatusCode::SERVICE_UNAVAILABLE {
                        let mut s = shared_state_clone.lock().await;
                        s.rate_limit_hits += 1;
                        break;
                    } else if !response.status().is_success() {
                        let mut s = shared_state_clone.lock().await;
                        s.global_error = Some(format!("Chunk {} failed: HTTP {}", chunk.id, response.status()));
                        break;
                    }

                    let mut current_offset = chunk.current;
                    let mut downloaded_for_this_thread = chunk.current - chunk.start;
                    let mut buf = bytes::BytesMut::with_capacity(4 * 1024 * 1024);

                    while let Some(chunk_bytes) = response.chunk().await.unwrap_or(None) {
                        if cancel_flag_clone.load(Ordering::Relaxed) {
                            let mut s = shared_state_clone.lock().await;
                            s.global_error = Some("Paused by user".to_string());
                            break;
                        }
                        
                        buf.extend_from_slice(&chunk_bytes);
                        
                        let current_end = {
                            let s = shared_state_clone.lock().await;
                            s.download_state.chunks.iter().find(|c| c.id == chunk.id).map(|c| c.end).unwrap_or(chunk.end)
                        };

                        if buf.len() >= 2 * 1024 * 1024 || current_offset + (buf.len() as u64) >= current_end {
                            let buf_to_write = buf.split().freeze();
                            let f_clone = file_clone.clone();
                            let offset_to_write = current_offset;
                            let len = buf_to_write.len() as u64;

                            tokio::task::spawn_blocking(move || {
                                let mut written = 0;
                                while written < buf_to_write.len() {
                                    if let Ok(n) = f_clone.write_at(&buf_to_write[written..], offset_to_write + written as u64) {
                                        written += n;
                                    } else {
                                        break;
                                    }
                                }
                            }).await.expect("Task panicked");

                            current_offset += len;
                            downloaded_for_this_thread += len;
                            
                            {
                                let mut s = shared_state_clone.lock().await;
                                if let Some(c) = s.download_state.chunks.iter_mut().find(|c| c.id == chunk.id) {
                                    c.current = current_offset;
                                }
                                let _ = StateManager::save(&state_file_path_clone, &s.download_state);
                            }

                            event_emitter_clone.emit_progress(ProgressPayload {
                                thread_id: chunk.id,
                                chunk_size: total_chunk_size,
                                bytes_downloaded: downloaded_for_this_thread,
                            });
                        }

                        if current_offset >= current_end {
                            break;
                        }
                    }

                    {
                        let mut s = shared_state_clone.lock().await;
                        s.active_chunk_ids.remove(&chunk.id);
                    }
                }
            });
            tasks.push(task);
        }

        for task in tasks {
            let _ = task.await;
        }

        let final_state = active_state.lock().await;
        if final_state.rate_limit_hits > 0 {
            return Err(format!("RATE_LIMIT"));
        }

        if let Some(ref err) = final_state.global_error {
            if err == "Paused by user" {
                return Err("Download Paused. Ready to Resume.".to_string());
            }
            return Err(err.clone());
        }

        let pending: Vec<_> = final_state.download_state.chunks.iter().filter(|c| c.current <= c.end).collect();
        if pending.is_empty() {
            let _ = std::fs::remove_file(&state_file_path);
            Ok(format!("Saved to {}", file_path.display()))
        } else {
            Err("Download incomplete".to_string())
        }
    }
}
