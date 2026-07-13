mod config;
mod progress;

use clap::Parser;
use config::Args;
use progress::IndicatifTracker;
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use boltfetch_core::downloader::Downloader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = Args::parse();
    
    let progress = Arc::new(IndicatifTracker::new());

    let downloader = Downloader::new(progress)?;
    
    let cancel_flag = Arc::new(AtomicBool::new(false));
    
    // We handle SIGINT (Ctrl+C) to gracefully cancel the download
    let cancel_flag_clone = cancel_flag.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel_flag_clone.store(true, Ordering::SeqCst);
    });

    match downloader.download(&args.url, None, &args.output, args.threads, cancel_flag).await {
        Ok(msg) => println!("\nSuccess! {}", msg),
        Err(e) => eprintln!("\nDownload failed: {}", e),
    }

    Ok(())
}