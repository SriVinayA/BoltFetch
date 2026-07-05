use crate::http::HttpFetcher;
use crate::file_io::FileWriter;
use crate::progress::ProgressTracker;
use crate::state::{ChunkState, StateManager, DownloadState};
use std::sync::Arc;
use std::error::Error;
use futures::future::join_all;
use std::path::Path;

struct ActiveState {
    pub download_state: DownloadState,
    pub active_chunk_ids: std::collections::HashSet<usize>,
}

impl ActiveState {
    fn get_work(&mut self) -> Option<ChunkState> {
        if let Some(c) = self.download_state.chunks.iter().find(|c| !self.active_chunk_ids.contains(&c.id) && c.current <= c.end) {
            self.active_chunk_ids.insert(c.id);
            return Some(c.clone());
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
            
            self.active_chunk_ids.insert(new_c.id);
            self.download_state.chunks.push(new_c.clone());
            return Some(new_c);
        }

        None
    }
}

pub struct Downloader<H: HttpFetcher, F: FileWriter, P: ProgressTracker> {
    http: Arc<H>,
    file: Arc<F>,
    progress: Arc<P>,
}

impl<H: HttpFetcher + 'static, F: FileWriter + 'static, P: ProgressTracker + 'static> Downloader<H, F, P> {
    pub fn new(http: Arc<H>, file: Arc<F>, progress: Arc<P>) -> Self {
        Self {
            http,
            file,
            progress,
        }
    }

    pub async fn download(&self, url: &str, num_threads: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
        let content_length = self.http.get_content_length(url).await?;
        
        if !self.http.check_range_support(url).await? {
            println!("Server does not support multipart downloads.");
            return Ok(());
        }

        println!("File size: {} bytes. Starting download with {} threads...\n", content_length, num_threads);

        self.file.pre_allocate(content_length)?;

        let state_file_path = Path::new("download.boltfetch");
        let (download_state, _is_resume) = StateManager::load_or_init(state_file_path, url, content_length);

        let shared_state = Arc::new(tokio::sync::Mutex::new(ActiveState {
            download_state,
            active_chunk_ids: std::collections::HashSet::new(),
        }));

        let mut tasks = Vec::new();

        for _ in 0..num_threads {
            let http_clone = self.http.clone();
            let file_clone = self.file.clone();
            let url_clone = url.to_string();
            let progress_clone = self.progress.clone();
            let shared_state_clone = shared_state.clone();
            
            let task = tokio::spawn(async move {
                loop {
                    let chunk = {
                        let mut s = shared_state_clone.lock().await;
                        match s.get_work() {
                            Some(c) => c,
                            None => break, // No more work
                        }
                    };

                    let size = chunk.end - chunk.start + 1;
                    let thread_progress = progress_clone.add_thread(chunk.id as u64, size, chunk.start, chunk.end);
                    // Fast forward progress to current
                    if chunk.current > chunk.start {
                        thread_progress.inc(chunk.current - chunk.start);
                    }
                    
                    let mut response = match http_clone.download_chunk(&url_clone, chunk.current, chunk.end).await {
                        Ok(res) => res,
                        Err(_) => {
                            // In a robust implementation we would retry, 
                            // but for now we just gracefully continue to let another thread pick it up.
                            {
                                let mut s = shared_state_clone.lock().await;
                                s.active_chunk_ids.remove(&chunk.id);
                            }
                            continue;
                        }
                    };
                    
                    let mut current_offset = chunk.current;
                    let mut buf = bytes::BytesMut::with_capacity(4 * 1024 * 1024);
                    
                    while let Some(chunk_bytes) = response.chunk().await.unwrap_or(None) {
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
                            thread_progress.inc(len);
                            
                            {
                                let mut s = shared_state_clone.lock().await;
                                if let Some(c) = s.download_state.chunks.iter_mut().find(|c| c.id == chunk.id) {
                                    c.current = current_offset;
                                }
                                let _ = StateManager::save(Path::new("download.boltfetch"), &s.download_state);
                            }
                        }
                        
                        if current_offset >= current_end {
                            break;
                        }
                    }
                    
                    thread_progress.finish(format!("Thread {} - Complete!", chunk.id));
                    
                    {
                        let mut s = shared_state_clone.lock().await;
                        s.active_chunk_ids.remove(&chunk.id);
                    }
                }
            });
            
            tasks.push(task);
        }

        join_all(tasks).await;
        let _ = std::fs::remove_file("download.boltfetch");

        Ok(())
    }
}
