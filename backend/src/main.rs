mod config;
mod downloader;
mod file_io;
mod http;
mod progress;

use clap::Parser;
use config::Args;
use downloader::Downloader;
use file_io::LocalFileWriter;
use http::ReqwestFetcher;
use progress::IndicatifTracker;
use std::error::Error;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    
    let http = Arc::new(ReqwestFetcher::new());
    let file = Arc::new(LocalFileWriter::new(&args.output)?);
    let progress = Arc::new(IndicatifTracker::new());

    let downloader = Downloader::new(http, file, progress);
    
    downloader.download(&args.url, args.threads).await?;

    println!("\nSuccess! Download complete: {}", args.output);

    Ok(())
}