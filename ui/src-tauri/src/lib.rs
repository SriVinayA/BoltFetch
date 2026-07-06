use futures::stream::{self, StreamExt};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize, Deserialize)]
struct ProgressPayload { chunk_id: usize, thread_id: usize, start: u64, current: u64, end: u64, thread_downloaded: u64, status: String }

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ChunkState { id: usize, start: u64, current: u64, end: u64 }

#[derive(Serialize, Deserialize, Clone)]
struct DownloadState { url: String, total_size: u64, chunks: Vec<ChunkState> }

pub struct AppState { pub cancel_flag: Arc<AtomicBool> }

#[tauri::command]
fn stop_download(state: tauri::State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

// --- NEW: A sleep command so the frontend can wait between auto-retries ---
#[tauri::command]
async fn sleep_delay(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
}

#[tauri::command]
async fn start_download(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    url: String,
    output: String,
    threads: u64,
) -> Result<String, String> {
    
    state.cancel_flag.store(false, Ordering::SeqCst);

    let client = Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let range_check = client.get(&url).header(RANGE, "bytes=0-0").send().await.map_err(|e| format!("Network error: {}", e))?;
    let supports_range = range_check.status() == StatusCode::PARTIAL_CONTENT;
    
    if !supports_range && range_check.status() != StatusCode::OK {
        return Err(format!("Server returned HTTP {}", range_check.status()));
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
    let _ = app.emit("filename-resolved", final_filename.clone());

    let content_length: u64 = if supports_range {
        range_check.headers().get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok()).ok_or("No Content-Range header")?
            .split('/').last().and_then(|s| s.parse().ok()).ok_or("Failed to parse size")?
    } else {
        range_check.headers().get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok()).unwrap_or(0)
    };

    let mut file_path = dirs::download_dir().ok_or("Could not find Downloads directory")?;
    file_path.push(&final_filename);
    let state_file_path = file_path.with_extension("boltfetch");

    let mut download_state = DownloadState { url: url.clone(), total_size: content_length, chunks: Vec::new() };
    let mut is_resume = false;

    if supports_range && state_file_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&state_file_path) {
            if let Ok(parsed) = serde_json::from_str::<DownloadState>(&content) {
                if parsed.url == url && parsed.total_size == content_length {
                    download_state = parsed;
                    is_resume = true;
                }
            }
        }
    }

    let actual_threads = if supports_range { threads } else { 1 };

    if !is_resume {
        let chunk_size = if actual_threads > 0 && content_length > 0 { content_length / actual_threads } else { 0 };
        for i in 0..(actual_threads as usize) {
            let start = (i as u64) * chunk_size;
            let end = if i == (actual_threads as usize) - 1 { if content_length > 0 { content_length - 1 } else { 0 } } else { (i as u64 + 1) * chunk_size - 1 };
            download_state.chunks.push(ChunkState { id: i, start, current: start, end });
        }
        let file = File::create(&file_path).map_err(|e| format!("Failed to create file: {}", e))?;
        file.set_len(content_length).map_err(|e| format!("Failed to allocate disk space: {}", e))?;
    } else if !file_path.exists() {
        return Err("Target file is missing. Please delete the .boltfetch file and start over.".into());
    }

    for chunk in &download_state.chunks {
        let _ = app.emit("download-progress", ProgressPayload {
            chunk_id: chunk.id,
            thread_id: chunk.id,
            start: chunk.start,
            current: chunk.current,
            end: chunk.end,
            thread_downloaded: chunk.current.saturating_sub(chunk.start),
            status: "Initializing...".to_string(),
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
        let client_clone = client.clone();
        let file_clone = file.clone();
        let url_clone = url.clone();
        let app_clone = app.clone();
        let cancel_flag = state.cancel_flag.clone();
        let shared_state_clone = shared_state.clone();
        let state_file_path_clone = state_file_path.clone();

        async move {
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
                if cancel_flag.load(Ordering::Relaxed) { return Err("Paused by user".to_string()); }

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
                    let _ = std::fs::write(&state_file_path_clone, serde_json::to_string(&*s).unwrap());
                }

                let _ = app_clone.emit("download-progress", ProgressPayload {
                    chunk_id: chunk.id,
                    thread_id: chunk.id,
                    start: chunk.start,
                    current: current_offset,
                    end: chunk.end,
                    thread_downloaded: downloaded_for_this_thread,
                    status: "Receiving data...".to_string(),
                });
            }
            Ok::<(), String>(())
        }
    });

    let stream = stream::iter(pending_futures).buffer_unordered(threads as usize);
    let results: Vec<Result<(), String>> = stream.collect().await;

    // --- NEW: Advanced Error Analysis ---
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
        // Calculate exactly how many threads were successfully used
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { cancel_flag: Arc::new(AtomicBool::new(false)) })
        // Make sure all 3 commands are registered!
        .invoke_handler(tauri::generate_handler![start_download, stop_download, sleep_delay]) 
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}