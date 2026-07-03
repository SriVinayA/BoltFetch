use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

// 1. Bind to Tauri's native JS APIs
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    // Add the listener API to catch backend events
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>) -> JsValue;
}

// 2. Define the payloads
#[derive(Serialize, Deserialize)]
struct DownloadArgs<'a> {
    url: &'a str,
    output: &'a str,
    threads: u64,
}

// This matches the struct we emit from the Tauri backend
#[derive(Clone, Serialize, Deserialize)]
struct ProgressPayload {
    thread_id: usize,
    chunk_size: u64,
    bytes_downloaded: u64,
}

// Tauri wraps emitted events in this object
#[derive(Deserialize)]
struct TauriEvent {
    payload: ProgressPayload,
}

#[component]
pub fn App() -> impl IntoView {
    let (url, set_url) = signal(String::from("https://proof.ovh.net/files/100Mb.dat"));
    let (output, set_output) = signal(String::from("test_video.mp4"));
    let (threads, set_threads) = signal(8u64);
    let (status, set_status) = signal(String::from("Ready to download."));
    
    // 3. A reactive map to store the progress of each thread
    let (progress, set_progress) = signal(HashMap::<usize, ProgressPayload>::new());

    // 4. Set up the event listener to catch backend progress
    Effect::new(move |_| {
        let handler = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: JsValue| {
            if let Ok(tauri_event) = serde_wasm_bindgen::from_value::<TauriEvent>(event) {
                set_progress.update(|map| {
                    map.insert(tauri_event.payload.thread_id, tauri_event.payload);
                });
            }
        }) as Box<dyn FnMut(JsValue)>);

        spawn_local(async move {
            listen("download-progress", &handler).await;
            handler.forget(); // Keep the closure alive in memory
        });
    });

    let download = move |_| {
        let u = url.get();
        let o = output.get();
        let t = threads.get();
        
        spawn_local(async move {
            // Reset progress bars on new download
            set_progress.set(HashMap::new());
            set_status.set("Downloading...".to_string());
            
            let args = DownloadArgs {
                url: &u,
                output: &o,
                threads: t,
            };
            
            let js_args = serde_wasm_bindgen::to_value(&args).unwrap();
            let res = invoke("start_download", js_args).await;
            
            set_status.set(res.as_string().unwrap_or_else(|| "Done".into()));
        });
    };

    view! {
        <main style="padding: 2rem; font-family: system-ui, sans-serif; display: flex; flex-direction: column; gap: 1rem; max-width: 500px; margin: 0 auto; background: #1e1e1e; color: white; height: 100vh;">
            <h1 style="text-align: center; color: #00e676; margin-bottom: 0;">"⚡ BoltFetch"</h1>
            
            <label style="font-size: 0.9rem; color: #aaa;">"Target URL"</label>
            <input
                placeholder="File URL"
                on:input=move |ev| set_url.set(event_target_value(&ev))
                prop:value=url
                style="padding: 0.75rem; font-size: 1rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;"
            />
            
            <label style="font-size: 0.9rem; color: #aaa;">"Output Filename"</label>
            <input
                placeholder="Output File Name"
                on:input=move |ev| set_output.set(event_target_value(&ev))
                prop:value=output
                style="padding: 0.75rem; font-size: 1rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;"
            />
            
            <label style="font-size: 0.9rem; color: #aaa;">"Concurrent Threads"</label>
            <input
                type="number"
                placeholder="Threads"
                on:input=move |ev| set_threads.set(event_target_value(&ev).parse().unwrap_or(8))
                prop:value=threads
                style="padding: 0.75rem; font-size: 1rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;"
            />
            
            <button
                on:click=download
                style="margin-top: 1rem; padding: 1rem; font-size: 1.1rem; font-weight: bold; background-color: #00e676; color: #121212; border: none; border-radius: 6px; cursor: pointer;"
            >
                "Start Download"
            </button>
            
            <p style="text-align: center; margin-top: 0.5rem; font-style: italic; font-size: 0.9rem; color: #00e676;">
                {move || status.get()}
            </p>

            // 5. The Dynamic Progress Bars
            <div style="display: flex; flex-direction: column; gap: 0.5rem; margin-top: 1rem; overflow-y: auto;">
                {move || {
                    let mut active_threads: Vec<_> = progress.get().into_values().collect();
                    active_threads.sort_by_key(|p| p.thread_id); // Keep them in order 0-8
                    
                    active_threads.into_iter().map(|p| {
                        let percentage = if p.chunk_size == 0 { 0.0 } else { (p.bytes_downloaded as f64 / p.chunk_size as f64) * 100.0 };
                        
                        view! {
                            <div style="background: #2a2a2a; padding: 0.5rem 0.75rem; border-radius: 6px; border: 1px solid #333;">
                                <div style="display: flex; justify-content: space-between; font-size: 0.8rem; margin-bottom: 0.4rem; color: #ddd;">
                                    <span>"Thread " {p.thread_id}</span>
                                    <span>{format!("{:.1}%", percentage)}</span>
                                </div>
                                <div style="background: #111; border-radius: 4px; overflow: hidden; height: 6px;">
                                    <div style=format!("background: #00e676; height: 100%; width: {}%; transition: width 0.1s linear;", percentage)></div>
                                </div>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
        </main>
    }
}