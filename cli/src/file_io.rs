use std::fs::File;
use std::os::unix::fs::FileExt;
use std::error::Error;
use std::sync::Arc;

pub trait FileWriter: Send + Sync {
    fn pre_allocate(&self, size: u64) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn write_at(&self, buf: &[u8], offset: u64) -> Result<usize, Box<dyn Error + Send + Sync>>;
}

pub struct LocalFileWriter {
    file: Arc<File>,
}

impl LocalFileWriter {
    pub fn new(path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let file = File::create(path)?;
        Ok(Self {
            file: Arc::new(file),
        })
    }
}

impl FileWriter for LocalFileWriter {
    fn pre_allocate(&self, size: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.file.set_len(size)?;
        Ok(())
    }

    fn write_at(&self, buf: &[u8], offset: u64) -> Result<usize, Box<dyn Error + Send + Sync>> {
        let bytes_written = self.file.write_at(buf, offset)?;
        Ok(bytes_written)
    }
}
