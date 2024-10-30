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
  const USAGE: &str = "Usage: vid-downloader -i <input_link> [-o <output_file>]";
  let args: Vec<String> = args().collect();
  if args.len() < 3 {
    println!("{}", USAGE);
    return Ok(());
  }

  let (input_link, output_name) = parse_input(args);
  if input_link.is_empty() {
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
    .navigate_to(&input_link)?
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
  downlaod_video(pure_m3u8_url, &output_name).expect("Failed to execute ffmpeg command");

  println!("Downloaded video successfully");
  Ok(())
}

fn parse_input(args: Vec<String>) -> (String, String) {
  let mut input_link = String::new();
  let mut output_name = String::from("video.mp4");

  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "-i" if i + 1 < args.len() => {
        input_link = args[i + 1].clone();
        i += 1;
      }
      "-o" if i + 1 < args.len() => {
        output_name = args[i + 1].clone();
        i += 1;
      }
      _ => {
        println!("Usage: vid-downloader -i <input_link> [-o <output_file>]");
        return (String::new(), String::new());
      }
    }
    i += 1;
  }

  if !output_name.ends_with(".mp4") {
    output_name.push_str(".mp4");
  }

  (input_link, output_name)
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

fn downlaod_video(url: &str, output_name: &str) -> Result<(), Box<dyn Error>> {
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
