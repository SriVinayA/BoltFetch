use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub trait ProgressTracker: Send + Sync {
    fn add_thread(&self, thread_id: u64, size: u64, start: u64, end: u64) -> Box<dyn ThreadProgress + Send + Sync>;
}

pub trait ThreadProgress: Send + Sync {
    fn inc(&self, delta: u64);
    fn finish(&self, msg: String);
}

pub struct IndicatifTracker {
    multi_progress: MultiProgress,
    style: ProgressStyle,
}

impl IndicatifTracker {
    pub fn new() -> Self {
        let style = ProgressStyle::with_template(
            "{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})"
        ).unwrap().progress_chars("#>-");
        
        Self {
            multi_progress: MultiProgress::new(),
            style,
        }
    }
}

pub struct IndicatifThread {
    pb: ProgressBar,
}

impl ThreadProgress for IndicatifThread {
    fn inc(&self, delta: u64) {
        self.pb.inc(delta);
    }

    fn finish(&self, msg: String) {
        self.pb.finish_with_message(msg);
    }
}

impl ProgressTracker for IndicatifTracker {
    fn add_thread(&self, thread_id: u64, size: u64, start: u64, end: u64) -> Box<dyn ThreadProgress + Send + Sync> {
        let pb = self.multi_progress.add(ProgressBar::new(size));
        pb.set_style(self.style.clone());
        pb.set_message(format!("Thread {} (bytes {}-{})", thread_id, start, end));
        Box::new(IndicatifThread { pb })
    }
}
