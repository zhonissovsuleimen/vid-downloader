use std::error::Error;
use std::io::Error as ioError;
use std::io::ErrorKind;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use headless_chrome::browser::tab::{RequestInterceptor, RequestPausedDecision};
use headless_chrome::browser::transport::{SessionId, Transport};
use headless_chrome::protocol::cdp::Fetch::events::RequestPausedEvent;
use headless_chrome::protocol::cdp::Fetch::{RequestPattern, RequestStage};
use headless_chrome::protocol::cdp::Network::ResourceType;
use headless_chrome::{Browser, LaunchOptions};

fn main() -> Result<(), Box<dyn Error>> {
  let test_link = r"https://x.com/shitpost_2077/status/1851260612161966480";
  let result = Arc::new(Mutex::new(String::new()));
  let result_clone = result.clone();

  println!("Starting browser");
  let browser = Browser::new(LaunchOptions {
    idle_browser_timeout: Duration::from_secs(60),
    args: vec![std::ffi::OsStr::new("--incognito")],
    ..Default::default()
  })?;

  println!("Opening new tab");
  let tab = browser.new_tab().expect("Failed to open tab");

  let interceptor = get_interceptor(result_clone);

  let pattern = RequestPattern {
    url_pattern: Some("https://video.twimg.com/ext_tw_video/*".to_string()),
    resource_Type: Some(ResourceType::Xhr),
    request_stage: Some(RequestStage::Request),
  };

  tab.enable_fetch(Some(&vec![pattern]), None)?;
  tab.enable_request_interception(interceptor)?;

  println!("Navigating to {}", test_link);
  tab
    .navigate_to(&test_link)?
    .wait_until_navigated()
    .expect("Failed to navigate to link");

  let m3u8_url = result.lock().unwrap().to_owned();
  println!("Found m3u8 url: {}", m3u8_url);
  let pure_m3u8_url = m3u8_url.split("?").collect::<Vec<&str>>()[0];

  println!("Executing ffmpeg command");
  downlaod_video(pure_m3u8_url).expect("Failed to execute ffmpeg command");

  println!("Downloaded video successfully");
  Ok(())
}

fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(
    move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.url.contains("tag=12") {
        let mut asd = result.lock().unwrap();
        *asd = event.params.request.url.to_owned();
      }

      RequestPausedDecision::Continue(None)
    },
  )
}

fn downlaod_video(url: &str) -> Result<(), Box<dyn Error>> {
  let exec = Command::new("ffmpeg")
    .arg("-y")
    .args(["-i", &url])
    .args(["-c", "copy"])
    .arg("output.mp4")
    .output()?;

  match exec.status.success() {
    true => Ok(()),
    false => Err(Box::new(ioError::new(ErrorKind::Other, "ffmpeg command failed"))),
  }
}
