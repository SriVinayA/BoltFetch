use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Mutex;
use std::collections::HashMap;
use boltfetch_core::events::{ProgressEmitter, ProgressPayload};

pub struct IndicatifTracker {
    multi_progress: MultiProgress,
    style: ProgressStyle,
    bars: Mutex<HashMap<usize, ProgressBar>>,
}

impl IndicatifTracker {
    pub fn new() -> Self {
        let style = ProgressStyle::with_template(
            "{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})"
        ).unwrap().progress_chars("#>-");
        
        Self {
            multi_progress: MultiProgress::new(),
            style,
            bars: Mutex::new(HashMap::new()),
        }
    }
}

impl ProgressEmitter for IndicatifTracker {
    fn emit_progress(&self, payload: ProgressPayload) {
        let mut bars = self.bars.lock().unwrap();
        
        // If we haven't created a progress bar for this chunk yet, create one
        let pb = bars.entry(payload.chunk_id).or_insert_with(|| {
            let size = payload.end - payload.start + 1;
            let pb = self.multi_progress.add(ProgressBar::new(size));
            pb.set_style(self.style.clone());
            pb.set_message(format!("Thread {} (bytes {}-{})", payload.thread_id, payload.start, payload.end));
            pb
        });
        
        pb.set_position(payload.current - payload.start);
        
        if payload.current >= payload.end {
            pb.finish_with_message(format!("Thread {} - Complete!", payload.thread_id));
        }
    }

    fn emit_filename_resolved(&self, filename: String) {
        println!("Downloading to: {}", filename);
    }
    
    fn emit_log(&self, msg: String) {
        println!("{}", msg);
    }
}
