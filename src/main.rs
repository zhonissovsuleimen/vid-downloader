use std::{
  env::args,
  error::Error,
  process::Command,
  sync::{Arc, Mutex},
  time::Duration,
};

use headless_chrome::browser::{
  tab::{RequestInterceptor, RequestPausedDecision},
  transport::{SessionId, Transport},
};
use headless_chrome::protocol::cdp::{
  Fetch::{events::RequestPausedEvent, RequestPattern, RequestStage},
  Network::ResourceType,
};
use headless_chrome::{Browser, LaunchOptions};

fn main() -> Result<(), Box<dyn Error>> {
  const USAGE: &str = "Usage: vid-downloader -i <input_link>";
  let args: Vec<String> = args().collect();
  if args.len() < 2 {
    println!("{}", USAGE);
    return Ok(());
  }

  let input_link = parse_input(args);
  if input_link.trim().is_empty() {
    println!("{}", USAGE);
    return Ok(());
  }

  println!("Starting browser");
  let browser = Browser::new(LaunchOptions {
    idle_browser_timeout: Duration::from_secs(60),
    args: vec![std::ffi::OsStr::new("--incognito")],
    ..Default::default()
  })?;

  println!("Opening new tab");
  let tab = browser.new_tab().expect("Failed to open tab");

  let result = Arc::new(Mutex::new(String::new()));
  let interceptor = get_interceptor(result.clone());

  let pattern = RequestPattern {
    url_pattern: Some("https://video.twimg.com/ext_tw_video/*".to_string()),
    resource_Type: Some(ResourceType::Xhr),
    request_stage: Some(RequestStage::Request),
  };

  tab.enable_fetch(Some(&vec![pattern]), None)?;
  tab.enable_request_interception(interceptor)?;

  
  println!("Navigating to {}", input_link);
  tab
    .navigate_to(&input_link)
    .expect("Invalid url")
    .wait_until_navigated()
    .expect("Failed to navigate to link");

  let m3u8_url = result.lock().unwrap().to_owned();
  if m3u8_url.is_empty() {
    println!("Failed to find m3u8 url");
    return Ok(());
  }
  println!("Found m3u8 url: {}", m3u8_url);
  
  println!("Executing ffmpeg command");
  let pure_m3u8_url = m3u8_url.split("?").collect::<Vec<&str>>()[0];
  downlaod_video(pure_m3u8_url).expect("Failed to execute ffmpeg command");

  println!("Downloaded video successfully");
  Ok(())
}

fn parse_input(args: Vec<String>) -> String {
  let mut input_link = String::new();

  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "-i" if i + 1 < args.len() => {
        input_link = args[i + 1].clone();
        i += 1;
      }
      _ => {
        return String::new();
      }
    }
    i += 1;
  }

  input_link
}

fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(
    move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.url.contains("tag=") {
        let mut asd = result.lock().unwrap();
        *asd = event.params.request.url.to_owned();
      }

      RequestPausedDecision::Continue(None)
    },
  )
}

fn downlaod_video(url: &str) -> Result<(), Box<dyn Error>> {
  let mut output_name = url.split('/').last().unwrap().split('.').collect::<Vec<&str>>()[0].to_string();
  output_name.push_str(".mp4");
  let output = Command::new("ffmpeg")
    .arg("-y")
    .args(["-i", &url])
    .args(["-c", "copy"])
    .arg(output_name)
    .output()?;

  if output.status.success() {
    return Ok(());
  } else {
    return Err(format!("ffmpeg command failed with status: {}", output.status).into());
  }
}
