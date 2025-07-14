use headless_chrome::{Browser, LaunchOptions};
use std::{sync::Arc, time::Duration};
use tracing::{info, error};

use crate::{
  downloader_error::DownloaderError,
  platforms::{tiktok::TiktokDownloader, twitter::TwitterDownloader},
};

#[derive(Clone)]
pub enum PreferredResolution {
  High,
  Medium,
  Low,
}

pub trait PlatformDownloader {
  async fn download(
    browser: Arc<Browser>, url: &str, preferred_resolution: Option<PreferredResolution>,
  ) -> Result<String, DownloaderError>;
  fn validate_url(url: &str) -> Result<(), DownloaderError>;
}

pub struct Downloader {
  browser: Arc<Browser>,
}

impl Downloader {
  pub fn new() -> Self {
    let browser = Browser::new(LaunchOptions {
      idle_browser_timeout: Duration::from_secs(1e7 as u64),
      args: vec![
        std::ffi::OsStr::new("--incognito"),
        std::ffi::OsStr::new("--mute-audio")
      ],
      ..Default::default()
    })
    .unwrap();

    let pid = browser.get_process_id().unwrap();
    let _ = ctrlc::set_handler(move || {
      use std::process::Command;

      #[cfg(target_os = "windows")]
      {
        let _ = Command::new("taskkill").args(&["/PID", &pid.to_string(), "/T", "/F"]).output();
      }
      #[cfg(target_os = "linux")]
      {
        let _ = Command::new("kill").args(&["-9", &format!("-{}", pid)]).output();
      }
      info!("Killed browser process (PID: {})", pid);
      info!("Shutting down...");
      std::process::exit(0);
    });

    Self { browser: Arc::new(browser) }
  }

  pub async fn download(&self, url: &str, preferred_resolution: Option<PreferredResolution>) -> Result<String, DownloaderError> {
    let url = url.trim_end();
    info!("Recieved download call: {url}");

    if !Self::is_url(url) {
      error!("Invalid input: {url}");
      return Err(DownloaderError::InvalidInputError);
    }

    let browser_clone = self.browser.clone();
    let result = match url {
      _ if TwitterDownloader::validate_url(url).is_ok() => TwitterDownloader::download(browser_clone, url, preferred_resolution).await,
      _ if TiktokDownloader::validate_url(url).is_ok() => TiktokDownloader::download(browser_clone, url, preferred_resolution).await,
      _ => Err(DownloaderError::UnsupportedPlatformError),
    };

    match result {
      Ok(output) => {
        info!("Downloaded completed for url: {url}");
        Ok(output)
      }
      Err(e) => {
        error!("Download failed for url: {url} ({e})");
        Err(e)
      }
    }
  }

  fn is_url(url: &str) -> bool {
    return !url.is_empty() && url.starts_with("https://");
  }
}
