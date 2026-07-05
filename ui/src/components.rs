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
        
        <div style="display: flex; flex-wrap: wrap; gap: 1rem;">
            <div style="flex: 1 1 200px; display: flex; flex-direction: column; gap: 0.5rem;">
                <label style="font-size: 0.9rem; color: #aaa;">"Output Filename"</label>
                <input placeholder="Output File Name" on:input=move |ev| set_output.set(event_target_value(&ev)) prop:value=output style="padding: 0.75rem; border-radius: 6px; border: 1px solid #444; background: #2a2a2a; color: white;" />
            </div>
            <div style="flex: 1 1 120px; display: flex; flex-direction: column; gap: 0.5rem;">
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
            <div style="display: flex; justify-content: space-between; margin-bottom: 0.5rem; font-size: 0.95rem; flex-wrap: wrap; gap: 0.5rem;">
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
pub fn UnifiedSegmentBar(
    chunks: ReadSignal<std::collections::HashMap<usize, ProgressPayload>>,
    global_total: ReadSignal<u64>,
) -> impl IntoView {
    view! {
        <div style="background: rgba(255, 255, 255, 0.05); padding: 1rem; border-radius: 12px; border: 1px solid rgba(255, 255, 255, 0.1); margin-top: 0.5rem; backdrop-filter: blur(10px);">
            <div style="margin-bottom: 0.75rem; font-size: 0.95rem; color: white; font-weight: bold;">"Segments"</div>
            <div style="position: relative; background: #111; border-radius: 8px; overflow: hidden; height: 32px; border: 1px solid rgba(255, 255, 255, 0.1); box-shadow: inset 0 2px 4px rgba(0,0,0,0.5);">
                {move || {
                    let total = global_total.get() as f64;
                    if total == 0.0 {
                        return ().into_any();
                    }
                    
                    let mut chunk_list: Vec<_> = chunks.get().into_values().collect();
                    chunk_list.sort_by_key(|c| c.chunk_id);
                    
                    chunk_list.into_iter().map(|c| {
                        let left = (c.start as f64 / total) * 100.0;
                        let width = ((c.end as f64 + 1.0 - c.start as f64) / total) * 100.0;
                        
                        let chunk_total = c.end as f64 + 1.0 - c.start as f64;
                        let downloaded = c.current as f64 - c.start as f64;
                        let inner_percent = if chunk_total <= 0.0 { 0.0 } else { (downloaded / chunk_total) * 100.0 };
                        
                        view! {
                            <div style=format!("position: absolute; left: {}%; width: {}%; height: 100%; border-right: 1px solid rgba(255,255,255,0.2); box-sizing: border-box;", left, width)>
                                <div style=format!("background: linear-gradient(90deg, #00e676, #00bfa5); height: 100%; width: {}%; transition: width 0.1s linear;", inner_percent)></div>
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </div>
        </div>
    }
}

#[component]
pub fn ConnectionsTable(
    threads_map: ReadSignal<std::collections::HashMap<usize, ProgressPayload>>,
) -> impl IntoView {
    view! {
        <div style="background: rgba(255, 255, 255, 0.05); border-radius: 12px; border: 1px solid rgba(255, 255, 255, 0.1); overflow: hidden; display: flex; flex-direction: column; backdrop-filter: blur(10px); flex: 1; margin-top: 0.5rem; min-height: 200px;">
            <div style="display: grid; grid-template-columns: 60px 1fr 2fr; padding: 0.75rem 1rem; background: rgba(0, 0, 0, 0.3); border-bottom: 1px solid rgba(255, 255, 255, 0.1); font-weight: bold; font-size: 0.85rem; color: #aaa;">
                <div>"No."</div>
                <div>"Downloaded"</div>
                <div>"Info"</div>
            </div>
            <div style="overflow-y: auto; flex: 1;">
                {move || {
                    let mut threads: Vec<_> = threads_map.get().into_values().collect();
                    threads.sort_by_key(|t| t.thread_id);
                    
                    if threads.is_empty() {
                        return view! {
                            <div style="padding: 2rem; text-align: center; color: #666; font-style: italic; font-size: 0.9rem;">
                                "No active connections"
                            </div>
                        }.into_any();
                    }
                    
                    threads.into_iter().map(|t| {
                        view! {
                            <div style="display: grid; grid-template-columns: 60px 1fr 2fr; padding: 0.75rem 1rem; border-bottom: 1px solid rgba(255, 255, 255, 0.05); font-size: 0.85rem; color: #eee; align-items: center; transition: background 0.2s;">
                                <div>{t.thread_id + 1}</div>
                                <div style="font-family: monospace;">{format_bytes(t.thread_downloaded as f64)}</div>
                                <div>
                                    <span style="background: rgba(0, 230, 118, 0.1); color: #00e676; padding: 0.2rem 0.5rem; border-radius: 4px; font-size: 0.75rem;">
                                        {t.status}
                                    </span>
                                </div>
                            </div>
                        }
                    }).collect_view().into_any()
                }}
            </div>
        </div>
    }
}
