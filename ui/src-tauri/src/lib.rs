use boltfetch_core::downloader::Downloader;
use boltfetch_core::events::{ProgressEmitter, ProgressPayload};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub cancel_flag: Arc<AtomicBool>,
}

struct TauriEmitter {
    app: AppHandle,
}

impl ProgressEmitter for TauriEmitter {
    fn emit_progress(&self, payload: ProgressPayload) {
        let _ = self.app.emit("download-progress", payload);
    }

    fn emit_filename_resolved(&self, filename: String) {
        let _ = self.app.emit("filename-resolved", filename);
    }

    fn emit_log(&self, _msg: String) {}
}

#[tauri::command]
fn stop_download(state: tauri::State<'_, AppState>) {
    state.cancel_flag.store(true, Ordering::SeqCst);
}

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

    let progress = Arc::new(TauriEmitter { app });
    let downloader = Downloader::new(progress)?;

    let download_dir = dirs::download_dir();

    // Core downloader handles the network drops, rate limits, work stealing, and saving to disk
    downloader
        .download(
            &url,
            download_dir,
            &output,
            threads,
            state.cancel_flag.clone(),
        )
        .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            stop_download,
            sleep_delay
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
