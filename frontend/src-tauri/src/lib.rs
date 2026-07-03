use futures::future::join_all;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::sync::Arc;
use tauri::{AppHandle, Emitter}; // Allows us to emit events to the frontend

// This struct defines the data we send to the frontend for the progress bar
#[derive(Clone, Serialize)]
struct ProgressPayload {
    thread_id: usize,
    chunk_size: u64,
    bytes_downloaded: u64,
}

#[tauri::command]
async fn start_download(
    app: AppHandle, // Injected by Tauri to handle events
    url: String,
    output: String,
    threads: u64,
) -> Result<String, String> { // We return Result so the frontend can catch errors
    let client = Client::new();

    // 1. Get total Content-Length
    let head_res = client.head(&url).send().await.map_err(|e| e.to_string())?;
    let content_length = head_res
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or("Could not get Content-Length from server")?;

    // 2. The Bulletproof Range Check
    let range_check = client
        .get(&url)
        .header(RANGE, "bytes=0-0")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if range_check.status() != StatusCode::PARTIAL_CONTENT {
        return Err("Server does not support multipart downloads.".into());
    }

    // 3. Pre-allocate the file on disk
    let file = File::create(&output).map_err(|e| e.to_string())?;
    file.set_len(content_length).map_err(|e| e.to_string())?;
    let file = Arc::new(file);

    let chunk_size = content_length / threads;
    let mut tasks = Vec::new();

    // 4. Spawn Concurrent Streaming Tasks
    for i in 0..(threads as usize) {
        let client_clone = client.clone();
        let file_clone = file.clone();
        let url_clone = url.clone();
        let app_clone = app.clone(); // Clone the app handle to emit events

        let start = (i as u64) * chunk_size;
        let end = if i == (threads as usize) - 1 {
            content_length - 1
        } else {
            (i as u64 + 1) * chunk_size - 1
        };
        let total_chunk_size = end - start + 1;

        let task = tokio::spawn(async move {
            let range_header = format!("bytes={}-{}", start, end);
            let mut response = client_clone
                .get(url_clone)
                .header(RANGE, range_header)
                .send()
                .await
                .expect("Failed to send request");

            let mut current_offset = start;
            let mut downloaded_for_this_thread = 0;

            while let Some(chunk) = response.chunk().await.expect("Failed to read chunk") {
                let mut chunk_offset = 0;

                while chunk_offset < chunk.len() {
                    let bytes_written = file_clone
                        .write_at(&chunk[chunk_offset..], current_offset)
                        .expect("Failed to write to disk");

                    chunk_offset += bytes_written;
                    current_offset += bytes_written as u64;
                    downloaded_for_this_thread += bytes_written as u64;
                }

                // EMIT PROGRESS TO FRONTEND
                let _ = app_clone.emit(
                    "download-progress",
                    ProgressPayload {
                        thread_id: i,
                        chunk_size: total_chunk_size,
                        bytes_downloaded: downloaded_for_this_thread,
                    },
                );
            }
        });

        tasks.push(task);
    }

    join_all(tasks).await;

    Ok(format!("Successfully downloaded to {}", output))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Register our custom command here!
        .invoke_handler(tauri::generate_handler![start_download])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}