use clap::Parser;
use reqwest::header::{CONTENT_LENGTH, RANGE};
use reqwest::{Client, StatusCode};
use std::fs::File;
use std::os::unix::fs::FileExt; 
use std::sync::Arc;
use std::error::Error;
use futures::future::join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

/// BoltFetch - A blazing fast multipart downloader
#[derive(Parser, Debug)]
#[command(name = "boltfetch")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The URL of the file to download
    url: String,

    /// The name of the output file
    #[arg(short, long, default_value = "downloaded_file.bin")]
    output: String,

    /// Number of concurrent downloading threads
    #[arg(short, long, default_value_t = 4)]
    threads: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    
    // Take ownership of the strings from args
    let url = args.url;
    let file_name = args.output;
    let num_threads = args.threads;

    let client = Client::new();

    // Pass references to the initial pre-flight checks
    let head_res = client.head(&url).send().await?;
    let content_length = head_res
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .expect("Could not get Content-Length from server");

    let range_check = client
        .get(&url)
        .header(RANGE, "bytes=0-0")
        .send()
        .await?;

    if range_check.status() != StatusCode::PARTIAL_CONTENT {
        println!("Server does not support multipart downloads.");
        return Ok(());
    }

    println!("File size: {} bytes. Starting download with {} threads...\n", content_length, num_threads);

    // Pass a reference to File::create
    let file = File::create(&file_name)?;
    file.set_len(content_length)?; 
    let file = Arc::new(file); 

    let multi_progress = MultiProgress::new();
    
    let progress_style = ProgressStyle::with_template(
        "{msg}\n[{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})"
    )
    .unwrap()
    .progress_chars("#>-");

    let chunk_size = content_length / num_threads;
    let mut tasks = Vec::new();

    for i in 0..num_threads {
        let client_clone = client.clone();
        let file_clone = file.clone(); 
        
        // CLONE THE URL HERE so the spawned task owns this string
        let url_clone = url.clone(); 
        
        let start = i * chunk_size;
        
        let end = if i == num_threads - 1 {
            content_length - 1
        } else {
            (i + 1) * chunk_size - 1
        };

        let total_chunk_size = end - start + 1;
        let pb = multi_progress.add(ProgressBar::new(total_chunk_size));
        pb.set_style(progress_style.clone());
        pb.set_message(format!("Thread {} (bytes {}-{})", i, start, end));

        // The async move block now perfectly captures the owned `url_clone`
        let task = tokio::spawn(async move {
            let range_header = format!("bytes={}-{}", start, end);
            
            let mut response = client_clone
                .get(url_clone) // Use the cloned string here
                .header(RANGE, range_header)
                .send()
                .await
                .expect("Failed to send request");

            let mut current_offset = start;
            
            while let Some(chunk) = response.chunk().await.expect("Failed to read network chunk") {
                let mut chunk_offset = 0;
                
                while chunk_offset < chunk.len() {
                    let bytes_written = file_clone
                        .write_at(&chunk[chunk_offset..], current_offset)
                        .expect("Failed to write chunk to disk");
                        
                    chunk_offset += bytes_written;
                    current_offset += bytes_written as u64;
                }
                pb.inc(chunk.len() as u64);
            }
            pb.finish_with_message(format!("Thread {} - Complete!", i));
        });

        tasks.push(task);
    }

    join_all(tasks).await;

    println!("\nSuccess! Download complete: {}", file_name);

    Ok(())
}