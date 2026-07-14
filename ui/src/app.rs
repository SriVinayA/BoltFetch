use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>) -> JsValue;
}

#[derive(Serialize, Deserialize)]
struct DownloadArgs<'a> { url: &'a str, output: &'a str, threads: u64 }

#[derive(Clone, Serialize, Deserialize)]
pub struct ProgressPayload { pub chunk_id: usize, pub thread_id: usize, pub start: u64, pub current: u64, pub end: u64, pub total_size: u64, pub thread_downloaded: u64, pub status: String }

#[derive(Deserialize)]
struct TauriEvent { payload: ProgressPayload }

#[derive(Deserialize)]
struct TauriFilenameEvent { payload: String }

pub fn format_bytes(bytes: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes;
    let mut i = 0;
    while val >= 1024.0 && i < units.len() - 1 { val /= 1024.0; i += 1; }
    format!("{:.2} {}", val, units[i])
}

pub fn format_time(seconds: f64) -> String {
    if seconds.is_infinite() || seconds.is_nan() || seconds <= 0.0 { return "--:--".to_string(); }
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 { format!("{:02}:{:02}:{:02}", hours, mins, secs) } else { format!("{:02}:{:02}", mins, secs) }
}

#[component]
pub fn App() -> impl IntoView {
    let (url, set_url) = signal(String::from("https://proof.ovh.net/files/100Mb.dat"));
    let (output, set_output) = signal(String::from("100Mb.dat"));
    let (threads, set_threads) = signal(8u64);
    let (status, set_status) = signal(String::from("Ready to download."));
    let (is_downloading, set_is_downloading) = signal(false); // NEW: State Tracker
    
    let (chunks, set_chunks) = signal(HashMap::<usize, ProgressPayload>::new());
    let (threads_map, set_threads_map) = signal(HashMap::<usize, ProgressPayload>::new());
    
    let (global_downloaded, set_global_downloaded) = signal(0u64);
    let (global_total, set_global_total) = signal(0u64);
    let (speed, set_speed) = signal(0.0f64);
    let (eta, set_eta) = signal(0.0f64);

    let (last_time, set_last_time) = signal(js_sys::Date::now());
    let (last_bytes, set_last_bytes) = signal(0u64);

    Effect::new(move |_| {
        let handler_progress = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
            if let Ok(tauri_event) = serde_wasm_bindgen::from_value::<TauriEvent>(event) {
                let payload = tauri_event.payload;
                set_chunks.update(|map| {
                    map.insert(payload.chunk_id, payload.clone());
                    
                    let current_total_bytes: u64 = map.values().map(|p| p.current.saturating_sub(p.start)).sum();
                    let current_total_size = payload.total_size;
                    
                    set_global_downloaded.set(current_total_bytes);
                    set_global_total.set(current_total_size);

                    let now = js_sys::Date::now();
                    let last_t = last_time.get_untracked();
                    let time_diff = (now - last_t) / 1000.0; 

                    if time_diff >= 0.25 { 
                        let last_b = last_bytes.get_untracked();
                        let bytes_diff = current_total_bytes.saturating_sub(last_b) as f64;
                        let current_speed = bytes_diff / time_diff; 
                        
                        set_speed.set(current_speed);
                        
                        if current_speed > 0.0 {
                            let remaining_bytes = current_total_size.saturating_sub(current_total_bytes) as f64;
                            set_eta.set(remaining_bytes / current_speed);
                        }

                        set_last_time.set(now);
                        set_last_bytes.set(current_total_bytes);
                    }
                });
                
                set_threads_map.update(|map| {
                    map.insert(payload.thread_id, payload);
                });
            }
        }) as Box<dyn FnMut(JsValue)>);

        let handler_filename = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
            if let Ok(tauri_event) = serde_wasm_bindgen::from_value::<TauriFilenameEvent>(event) {
                set_output.set(tauri_event.payload);
            }
        }) as Box<dyn FnMut(JsValue)>);

        spawn_local(async move {
            listen("download-progress", &handler_progress).await;
            listen("filename-resolved", &handler_filename).await;
            handler_progress.forget(); 
            handler_filename.forget();
        });
    });

    let toggle_download = move |_| {
        if is_downloading.get() {
            // TRIGGERS MANUAL PAUSE
            spawn_local(async move {
                let _ = invoke("stop_download", JsValue::NULL).await;
                set_status.set("Pausing download...".to_string());
                set_is_downloading.set(false);
            });
        } else {
            // TRIGGERS START / AUTO-RESUME
            let u = url.get();
            let o = output.get();
            let mut t = threads.get(); 
            
            spawn_local(async move {
                set_is_downloading.set(true);
                
                // --- THE AUTONOMOUS ORCHESTRATOR LOOP ---
                loop {
                    set_chunks.set(HashMap::new()); 
                    set_threads_map.set(HashMap::new());
                    set_global_downloaded.set(0);
                    set_global_total.set(0);
                    set_speed.set(0.0);
                    set_eta.set(0.0);
                    set_last_time.set(js_sys::Date::now());
                    set_last_bytes.set(0);
                    
                    set_status.set(format!("Connecting ({} threads)...", t));
                    
                    let args = DownloadArgs { url: &u, output: &o, threads: t };
                    let js_args = serde_wasm_bindgen::to_value(&args).unwrap();
                    
                    match invoke("start_download", js_args).await {
                        Ok(res) => {
                            // Download Complete!
                            set_eta.set(0.0);
                            set_speed.set(0.0);
                            set_status.set(res.as_string().unwrap_or_else(|| "Done".into()));
                            set_is_downloading.set(false);
                            break; 
                        }
                        Err(err) => {
                            let error_msg = err.as_string().unwrap_or_else(|| "Unknown error".into());
                            
                            // 1. Did the user click pause manually?
                            if error_msg.contains("Download Paused") || !is_downloading.get() {
                                set_status.set("Download Paused. Ready to Resume.".into());
                                set_is_downloading.set(false);
                                break;
                            }
                            
                            // 2. Did the server limit us at the start?
                            if error_msg.starts_with("RATE_LIMIT:") {
                                let parts: Vec<&str> = error_msg.split(':').collect();
                                let survived: u64 = parts.get(1).unwrap_or(&"0").parse().unwrap_or(0);
                                
                                if survived > 0 && survived < t {
                                    t = survived; 
                                } else {
                                    t = t.saturating_sub(1).max(1);
                                }
                                
                                set_threads.set(t); 
                                set_status.set(format!("Server limit hit! Auto-resuming with {} unused thread(s) in 3 seconds...", t));
                                
                                #[derive(Serialize, Deserialize)]
                                struct SleepArgs { ms: u64 }
                                let sleep_args = serde_wasm_bindgen::to_value(&SleepArgs { ms: 3000 }).unwrap();
                                let _ = invoke("sleep_delay", sleep_args).await;
                                
                                if !is_downloading.get() {
                                    set_status.set("Download Paused. Ready to Resume.".into());
                                    break;
                                }
                                
                                continue; 
                            }

                            // 3. NEW: Did the server randomly sever a connection mid-download?
                            if error_msg.contains("lost connection") || error_msg.contains("failed to connect") || error_msg.contains("error decoding") {
                                set_status.set("Network drop detected! Auto-resuming in 3 seconds...".into());
                                
                                #[derive(Serialize, Deserialize)]
                                struct SleepArgs { ms: u64 }
                                let sleep_args = serde_wasm_bindgen::to_value(&SleepArgs { ms: 3000 }).unwrap();
                                let _ = invoke("sleep_delay", sleep_args).await;
                                
                                if !is_downloading.get() {
                                    set_status.set("Download Paused. Ready to Resume.".into());
                                    break;
                                }
                                
                                // Loop back to the top! The .boltfetch file ensures we don't lose progress.
                                continue; 
                            }
                            
                            // 4. For any other fatal errors (like Disk Full, 404 Not Found)
                            set_status.set(format!("❌ {}", error_msg));
                            set_is_downloading.set(false);
                            set_speed.set(0.0);
                            set_eta.set(0.0);
                            break;
                        }
                    }
                }
            });
        }
    };

    view! {
        <main style="padding: max(1rem, 3vw); font-family: system-ui, sans-serif; display: flex; flex-direction: column; gap: 0.8rem; max-width: 800px; width: 100%; box-sizing: border-box; margin: 0 auto; background: #1e1e1e; color: white; height: 100vh;">
            <crate::components::ConfigForm 
                url=url set_url=set_url
                output=output set_output=set_output
                threads=threads set_threads=set_threads
            />
            
            <button on:click=toggle_download style=move || format!("margin-top: 0.5rem; padding: 1rem; font-size: 1.1rem; font-weight: bold; background-color: {}; color: #121212; border: none; border-radius: 6px; cursor: pointer;", if is_downloading.get() { "#ff9800" } else { "#00e676" })>
                {move || if is_downloading.get() { "Pause Download" } else { "Start / Resume" }}
            </button>
            <p style="text-align: center; font-style: italic; font-size: 0.9rem; color: #00e676; margin: 0;">{move || status.get()}</p>

            <crate::components::GlobalProgressView 
                global_total=global_total
                global_downloaded=global_downloaded
                speed=speed
                eta=eta
            />

            <crate::components::UnifiedSegmentBar chunks=chunks global_total=global_total />
            <crate::components::ConnectionsTable threads_map=threads_map />
        </main>
    }
}