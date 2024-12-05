use std::sync::{Arc, Mutex};
use headless_chrome::Browser;
use tokio::{ 
  signal,
  process::Command
};

enum DownloaderError {
  InvalidInput,
  UnsupportedPlatform
}


pub struct Downloader {
  browser: Arc<Browser>,
  process_id: u32
}

impl Downloader {
  pub fn new() -> Self {
    let browser = Arc::new(Browser::default().unwrap());
    let process_id = browser.get_process_id().unwrap();

    //spawn task that kills the browser when ctr-c command is issued
    tokio::spawn(async move {
      let _ = signal::ctrl_c().await;
      let _ = Command::new("taskkill").args(&["/F", "/PID", &process_id.to_string()]).output().await;
    });

    Self { browser, process_id }
  }

  pub async fn download(&self, url: &str) -> Result<(), DownloaderError> {
    Ok(())
  }
}
