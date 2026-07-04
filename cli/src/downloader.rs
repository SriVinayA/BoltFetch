use crate::http::HttpFetcher;
use crate::file_io::FileWriter;
use crate::progress::ProgressTracker;
use std::sync::Arc;
use std::error::Error;
use futures::future::join_all;

pub struct Downloader<H: HttpFetcher, F: FileWriter, P: ProgressTracker> {
    http: Arc<H>,
    file: Arc<F>,
    progress: Arc<P>,
}

impl<H: HttpFetcher + 'static, F: FileWriter + 'static, P: ProgressTracker + 'static> Downloader<H, F, P> {
    pub fn new(http: Arc<H>, file: Arc<F>, progress: Arc<P>) -> Self {
        Self {
            http,
            file,
            progress,
        }
    }

    pub async fn download(&self, url: &str, num_threads: u64) -> Result<(), Box<dyn Error>> {
        let content_length = self.http.get_content_length(url).await?;
        
        if !self.http.check_range_support(url).await? {
            println!("Server does not support multipart downloads.");
            return Ok(());
        }

        println!("File size: {} bytes. Starting download with {} threads...\n", content_length, num_threads);

        self.file.pre_allocate(content_length)?;

        let chunk_size = content_length / num_threads;
        let mut tasks = Vec::new();

        for i in 0..num_threads {
            let http_clone = self.http.clone();
            let file_clone = self.file.clone();
            let url_clone = url.to_string();
            
            let start = i * chunk_size;
            let end = if i == num_threads - 1 {
                content_length - 1
            } else {
                (i + 1) * chunk_size - 1
            };

            let size = end - start + 1;
            
            // Note: add_thread returns Box<dyn ThreadProgress> which might not be Send if we don't specify it.
            // But wait, the trait says ThreadProgress: Send + Sync. So Box<dyn ThreadProgress> is just that, but wait, it should be Box<dyn ThreadProgress + Send + Sync> to be safe in async move.
            // Oh, since the trait definition has `trait ThreadProgress: Send + Sync`, returning `Box<dyn ThreadProgress>` implies it might not have the bounds explicitly. Actually we should change the return type in the trait to `Box<dyn ThreadProgress>`. Wait, `dyn ThreadProgress` does NOT imply `Send`. 
            // It's safer to use `Box<dyn ThreadProgress>` if it doesn't cause issues, or just `let mut thread_progress = ...`. Let's see if rustc complains.
            let mut thread_progress = self.progress.add_thread(i, size, start, end);
            
            let task = tokio::spawn(async move {
                let mut response = http_clone.download_chunk(&url_clone, start, end).await.expect("Failed to get chunk");
                
                let mut current_offset = start;
                
                while let Some(chunk) = response.chunk().await.expect("Failed to read network chunk") {
                    let mut chunk_offset = 0;
                    
                    while chunk_offset < chunk.len() {
                        let bytes_written = file_clone.write_at(&chunk[chunk_offset..], current_offset).expect("Failed to write chunk");
                        chunk_offset += bytes_written;
                        current_offset += bytes_written as u64;
                    }
                    thread_progress.inc(chunk.len() as u64);
                }
                thread_progress.finish(format!("Thread {} - Complete!", i));
            });
            
            tasks.push(task);
        }

        join_all(tasks).await;

        Ok(())
    }
}
