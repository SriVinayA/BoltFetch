use leptos::prelude::*;
use crate::app::{format_bytes, format_time, ProgressPayload};

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <h1 style="text-align: center; color: #00e676; margin-bottom: 0;">"⚡ BoltFetch"</h1>
    }
}

#[component]
pub fn ConfigForm(
    url: ReadSignal<String>,
    set_url: WriteSignal<String>,
    output: ReadSignal<String>,
    set_output: WriteSignal<String>,
    threads: ReadSignal<u64>,
    set_threads: WriteSignal<u64>,
) -> impl IntoView {
    view! {
        <label style="font-size: 0.9rem; color: #aaa;">"Target URL"</label>
        <input 
            placeholder="File URL" 
            on:input=move |ev| {
                let new_url = event_target_value(&ev);
                set_url.set(new_url.clone());
                
                if let Some(file_segment) = new_url.split('/').last() {
                    let clean_name = file_segment.split('?').next().unwrap_or("download.bin");
                    if !clean_name.is_empty() { set_output.set(clean_name.to_string()); }
                }
            }
            prop:value=url 
            style="padding: 0.75rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;" 
        />
        
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
    }
}

#[component]
pub fn GlobalProgressView(
    global_total: ReadSignal<u64>,
    global_downloaded: ReadSignal<u64>,
    speed: ReadSignal<f64>,
    eta: ReadSignal<f64>,
) -> impl IntoView {
    view! {
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
    }
}

#[component]
pub fn ThreadProgressList(
    progress: ReadSignal<std::collections::HashMap<usize, ProgressPayload>>
) -> impl IntoView {
    view! {
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
    }
}
