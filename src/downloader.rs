use headless_chrome::{Browser, LaunchOptions};
use std::{sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tracing::info;

use crate::{downloader_error::DownloaderError, platforms::twitter::TwitterDownloader};

#[derive(Clone)]
pub enum PreferredResolution {
  High,
  Medium,
  Low,
}

pub trait PlatformDownloader {
  async fn download(
    browser: Arc<Mutex<Browser>>, url: &str, preferred_resolution: Option<PreferredResolution>,
  ) -> Result<String, DownloaderError>;
  fn validate_url(url: &str) -> Result<(), DownloaderError>;
}

pub struct Downloader {
  browser: Arc<Mutex<Browser>>,
}

impl Downloader {
  pub fn new() -> Self {
    let browser = Browser::new(LaunchOptions {
        idle_browser_timeout: Duration::from_secs(1e7 as u64),
        args: vec![std::ffi::OsStr::new("--incognito")],
        ..Default::default()
      })
    .unwrap();
    let process_id = browser.get_process_id().unwrap();

    //hopefully killing the browser if the terminal is terminated unexpectedly
    #[cfg(target_os = "windows")]
    unsafe {
      use std::ptr::null_mut;
      use winapi::um::handleapi::CloseHandle;
      use winapi::um::jobapi2::{AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject};
      use winapi::um::processthreadsapi::OpenProcess;
      use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
      };

      let h_job = CreateJobObjectW(null_mut(), null_mut());
      let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: std::mem::zeroed(),
        IoInfo: std::mem::zeroed(),
        ProcessMemoryLimit: 0,
        JobMemoryLimit: 0,
        PeakProcessMemoryUsed: 0,
        PeakJobMemoryUsed: 0,
      };
      info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
      SetInformationJobObject(
        h_job,
        JobObjectExtendedLimitInformation,
        &mut info as *mut _ as *mut _,
        std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
      );

      let process_handle = OpenProcess(0x001F0FFF, 0, process_id);
      AssignProcessToJobObject(h_job, process_handle);
      // Close the process handle when done
      CloseHandle(process_handle);
    }
    #[cfg(target_os = "linux")]
    {
      tokio::spawn(async move {
        info!("Waiting for ctrl-c command to kill browser");
        let _ = signal::ctrl_c().await;
        info!("Received ctrl-c command, killing browser");
        {
          let _ = Command::new("kill").args(&["-9", &process_id.to_string()]).output().await;
        }
      });
    }

    Self { browser: Arc::new(Mutex::new(browser)) }
  }

  pub async fn download(&self, url: &str, preferred_resolution: Option<PreferredResolution>) -> Result<String, DownloaderError> {
    info!("Recieved download call: {url}");

    match url {
      _ if TwitterDownloader::validate_url(url).is_ok() => {
        TwitterDownloader::download(self.browser.clone(), url, preferred_resolution).await
      },
      _ if Self::is_url(url) => {
        Err(DownloaderError::UnsupportedPlatformError)
      },
      _ => {
        Err(DownloaderError::InvalidInputError)
      },
    }
  }

  fn is_url(url: &str) -> bool {
    return !url.is_empty() && url.starts_with("https://");
  }
}
