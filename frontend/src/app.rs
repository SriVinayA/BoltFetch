use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// --- Native API Bindings ---
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>) -> JsValue;
}

// --- Payloads ---
#[derive(Serialize, Deserialize)]
struct DownloadArgs<'a> {
    url: &'a str,
    output: &'a str,
    threads: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct ProgressPayload {
    thread_id: usize,
    chunk_size: u64,
    bytes_downloaded: u64,
}

#[derive(Deserialize)]
struct TauriEvent {
    payload: ProgressPayload,
}

// --- Helper Functions ---
fn format_bytes(bytes: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut val = bytes;
    let mut i = 0;
    while val >= 1024.0 && i < units.len() - 1 {
        val /= 1024.0;
        i += 1;
    }
    format!("{:.2} {}", val, units[i])
}

fn format_time(seconds: f64) -> String {
    if seconds.is_infinite() || seconds.is_nan() || seconds <= 0.0 {
        return "--:--".to_string();
    }
    let total_secs = seconds as u64;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

// --- Main App Component ---
#[component]
pub fn App() -> impl IntoView {
    // Basic Form State
    let (url, set_url) = signal(String::from("https://proof.ovh.net/files/100Mb.dat"));
    let (output, set_output) = signal(String::from("test_video.mp4"));
    let (threads, set_threads) = signal(8u64);
    let (status, set_status) = signal(String::from("Ready to download."));
    
    // Detailed Progress State
    let (progress, set_progress) = signal(HashMap::<usize, ProgressPayload>::new());
    
    // Global Dashboard State
    let (global_downloaded, set_global_downloaded) = signal(0u64);
    let (global_total, set_global_total) = signal(0u64);
    let (speed, set_speed) = signal(0.0f64);
    let (eta, set_eta) = signal(0.0f64);

    // Internal state for math (untracked by the UI)
    let (last_time, set_last_time) = signal(js_sys::Date::now());
    let (last_bytes, set_last_bytes) = signal(0u64);

    // Event Listener setup
    Effect::new(move |_| {
        let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
            if let Ok(tauri_event) = serde_wasm_bindgen::from_value::<TauriEvent>(event) {
                set_progress.update(|map| {
                    map.insert(tauri_event.payload.thread_id, tauri_event.payload);
                    
                    // 1. Calculate the total downloaded bytes across all threads
                    let current_total_bytes: u64 = map.values().map(|p| p.bytes_downloaded).sum();
                    let current_total_size: u64 = map.values().map(|p| p.chunk_size).sum();
                    
                    set_global_downloaded.set(current_total_bytes);
                    set_global_total.set(current_total_size);

                    // 2. Calculate Speed & ETA 
                    let now = js_sys::Date::now();
                    let last_t = last_time.get_untracked();
                    let time_diff = (now - last_t) / 1000.0; // convert ms to seconds

                    // Only update the speed calculation every 250ms to keep the UI smooth
                    if time_diff >= 0.25 { 
                        let last_b = last_bytes.get_untracked();
                        let bytes_diff = current_total_bytes.saturating_sub(last_b) as f64;
                        let current_speed = bytes_diff / time_diff; // bytes per second
                        
                        set_speed.set(current_speed);
                        
                        if current_speed > 0.0 {
                            let remaining_bytes = current_total_size.saturating_sub(current_total_bytes) as f64;
                            set_eta.set(remaining_bytes / current_speed);
                        }

                        // Store current values for the next tick
                        set_last_time.set(now);
                        set_last_bytes.set(current_total_bytes);
                    }
                });
            }
        }) as Box<dyn FnMut(JsValue)>);

        spawn_local(async move {
            listen("download-progress", &handler).await;
            handler.forget(); 
        });
    });

    let download = move |_| {
        let u = url.get();
        let o = output.get();
        let t = threads.get();
        
        spawn_local(async move {
            // Reset all state for a fresh download
            set_progress.set(HashMap::new());
            set_global_downloaded.set(0);
            set_global_total.set(0);
            set_speed.set(0.0);
            set_eta.set(0.0);
            set_last_time.set(js_sys::Date::now());
            set_last_bytes.set(0);
            
            set_status.set("Downloading...".to_string());
            
            let args = DownloadArgs { url: &u, output: &o, threads: t };
            let js_args = serde_wasm_bindgen::to_value(&args).unwrap();
            
            let res = invoke("start_download", js_args).await;
            
            // On complete, force ETA to 0 and format completion message
            set_eta.set(0.0);
            set_speed.set(0.0);
            set_status.set(res.as_string().unwrap_or_else(|| "Done".into()));
        });
    };

    view! {
        <main style="padding: 2rem; font-family: system-ui, sans-serif; display: flex; flex-direction: column; gap: 0.8rem; max-width: 550px; margin: 0 auto; background: #1e1e1e; color: white; height: 100vh;">
            <h1 style="text-align: center; color: #00e676; margin-bottom: 0;">"⚡ BoltFetch"</h1>
            
            <label style="font-size: 0.9rem; color: #aaa;">"Target URL"</label>
            <input placeholder="File URL" on:input=move |ev| set_url.set(event_target_value(&ev)) prop:value=url style="padding: 0.75rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;" />
            
            <div style="display: flex; gap: 1rem;">
                <div style="flex: 1; display: flex; flex-direction: column; gap: 0.5rem;">
                    <label style="font-size: 0.9rem; color: #aaa;">"Output Filename"</label>
                    <input placeholder="Output File Name" on:input=move |ev| set_output.set(event_target_value(&ev)) prop:value=output style="padding: 0.75rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;" />
                </div>
                <div style="width: 120px; display: flex; flex-direction: column; gap: 0.5rem;">
                    <label style="font-size: 0.9rem; color: #aaa;">"Threads"</label>
                    <input type="number" on:input=move |ev| set_threads.set(event_target_value(&ev).parse().unwrap_or(8)) prop:value=threads style="padding: 0.75rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;" />
                </div>
            </div>
            
            <button on:click=download style="margin-top: 0.5rem; padding: 1rem; font-size: 1.1rem; font-weight: bold; background-color: #00e676; color: #121212; border: none; border-radius: 6px; cursor: pointer;">
                "Start Download"
            </button>
            <p style="text-align: center; font-style: italic; font-size: 0.9rem; color: #00e676; margin: 0;">{move || status.get()}</p>

            // --- The New Master Global Progress Dashboard ---
            <div style="background: #252525; padding: 1rem; border-radius: 8px; border: 1px solid #333; margin-top: 0.5rem;">
                <div style="display: flex; justify-content: space-between; margin-bottom: 0.5rem; font-size: 0.95rem;">
                    <span style="color: white; font-weight: bold;">"Global Progress"</span>
                    <span style="color: #00e676; font-family: monospace;">
                        {move || format!("{}/s", format_bytes(speed.get()))} " | ETA: " {move || format_time(eta.get())}
                    </span>
                </div>
                <div style="background: #111; border-radius: 6px; overflow: hidden; height: 14px; margin-bottom: 0.5rem;">
                    <div style=move || {
                        let total = global_total.get() as f64;
                        let downloaded = global_downloaded.get() as f64;
                        let percentage = if total == 0.0 { 0.0 } else { (downloaded / total) * 100.0 };
                        format!("background: #00e676; height: 100%; width: {}%; transition: width 0.2s linear;", percentage)
                    }></div>
                </div>
                <div style="text-align: right; font-size: 0.85rem; color: #aaa;">
                    {move || format!("{} / {}", format_bytes(global_downloaded.get() as f64), format_bytes(global_total.get() as f64))}
                </div>
            </div>

            // Individual Thread Progress Bars
            <div style="display: flex; flex-direction: column; gap: 0.5rem; overflow-y: auto; padding-right: 5px;">
                {move || {
                    let mut active_threads: Vec<_> = progress.get().into_values().collect();
                    active_threads.sort_by_key(|p| p.thread_id);
                    
                    active_threads.into_iter().map(|p| {
                        let percentage = if p.chunk_size == 0 { 0.0 } else { (p.bytes_downloaded as f64 / p.chunk_size as f64) * 100.0 };
                        view! {
                            <div style="background: #2a2a2a; padding: 0.4rem 0.6rem; border-radius: 6px; border: 1px solid #333;">
                                <div style="display: flex; justify-content: space-between; font-size: 0.75rem; margin-bottom: 0.3rem; color: #ccc;">
                                    <span>"Thread " {p.thread_id}</span>
                                    <span>{format!("{:.1}%", percentage)}</span>
                                </div>
                                <div style="background: #111; border-radius: 4px; overflow: hidden; height: 4px;">
                                    <div style=format!("background: #00e676; height: 100%; width: {}%;", percentage)></div>
                                </div>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
        </main>
    }
}