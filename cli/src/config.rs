use clap::Parser;

/// BoltFetch - A blazing fast multipart downloader
#[derive(Parser, Debug)]
#[command(name = "boltfetch")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// The URL of the file to download
    pub url: String,

    /// The name of the output file
    #[arg(short, long, default_value = "downloaded_file.bin")]
    pub output: String,

    /// Number of concurrent downloading threads
    #[arg(short, long, default_value_t = 4)]
    pub threads: u64,
}
