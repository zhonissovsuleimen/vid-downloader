#[derive(Debug)]
pub enum DownloaderError {
  InvalidInput,
  UnsupportedPlatform,
  Other(String)
}

impl From<anyhow::Error> for DownloaderError {
  fn from(e: anyhow::Error) -> Self {
    DownloaderError::Other(e.to_string())
  }
}