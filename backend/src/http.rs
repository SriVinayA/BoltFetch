use reqwest::{Client, StatusCode};
use reqwest::header::{CONTENT_LENGTH, RANGE};
use std::error::Error;

pub trait HttpFetcher: Send + Sync {
    fn get_content_length(&self, url: &str) -> impl std::future::Future<Output = Result<u64, Box<dyn Error>>> + Send;
    fn check_range_support(&self, url: &str) -> impl std::future::Future<Output = Result<bool, Box<dyn Error>>> + Send;
    fn download_chunk(&self, url: &str, start: u64, end: u64) -> impl std::future::Future<Output = Result<reqwest::Response, Box<dyn Error>>> + Send;
}

pub struct ReqwestFetcher {
    client: Client,
}

impl ReqwestFetcher {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

impl HttpFetcher for ReqwestFetcher {
    async fn get_content_length(&self, url: &str) -> Result<u64, Box<dyn Error>> {
        let head_res = self.client.head(url).send().await?;
        let content_length = head_res
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or("Could not get Content-Length from server")?;
        Ok(content_length)
    }

    async fn check_range_support(&self, url: &str) -> Result<bool, Box<dyn Error>> {
        let range_check = self.client
            .get(url)
            .header(RANGE, "bytes=0-0")
            .send()
            .await?;
        Ok(range_check.status() == StatusCode::PARTIAL_CONTENT)
    }

    async fn download_chunk(&self, url: &str, start: u64, end: u64) -> Result<reqwest::Response, Box<dyn Error>> {
        let range_header = format!("bytes={}-{}", start, end);
        let response = self.client
            .get(url)
            .header(RANGE, range_header)
            .send()
            .await?;
        Ok(response)
    }
}
