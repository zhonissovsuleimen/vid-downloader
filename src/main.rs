use std::{
  env::args,
  error::Error,
  process::Command,
  sync::{Arc, Mutex},
  time::Duration,
  io::{self, Write},
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

struct InputArgs {
  url: String,
  keep_alive: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
  const USAGE: &str = "Usage: vid-downloader [options]\nOptions:\n  -i --input: input url\n  -a --keep-alive: keep handling incoming links\n";
  let args: Vec<String> = args().collect();
  let mut input = parse_input(args);
  
  if input.url.is_empty() && !input.keep_alive {
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
    url_pattern: Some("https://video.twimg.com/*_video/*".to_string()),
    resource_Type: Some(ResourceType::Xhr),
    request_stage: Some(RequestStage::Request),
  };

  tab.enable_fetch(Some(&vec![pattern]), None)?;
  tab.enable_request_interception(interceptor)?;
  println!("Caching twitter");
  tab.navigate_to("https://x.com/jack/status/20").expect("Failed to open twitter").wait_until_navigated().expect("Failed to navigate to twitter");

  loop {
    if input.url.is_empty() {
      print!("Enter a url: ");
      io::stdout().flush().unwrap();
      let mut url = String::new();
      io::stdin().read_line(&mut url).expect("Failed to read line");
      input.url = url.trim().to_string();
    }

    println!("Navigating to {}", input.url);
    if tab.navigate_to(&input.url).is_err() {
      println!("Failed to navigate to link");
      input.url.clear();
      continue;
    }
    tab.wait_until_navigated()?;

    let m3u8_url = result.lock().unwrap().to_owned();
    if m3u8_url.is_empty() {
      println!("Failed to find m3u8 url");
      input.url.clear();
      continue;
    }
    println!("Found m3u8 url: {}", m3u8_url);

    println!("Executing ffmpeg command");
    let pure_m3u8_url = m3u8_url.split("?").collect::<Vec<&str>>()[0];
    download_video(pure_m3u8_url).expect("Failed to execute ffmpeg command");

    println!("Downloaded video successfully");

    input.url.clear();
    result.lock().unwrap().clear();
    if !input.keep_alive {
      break;
    }
  }
  Ok(())
}

fn parse_input(args: Vec<String>) -> InputArgs {
  let mut input = InputArgs {
    url: String::new(),
    keep_alive: false,
  };

  let mut i = 1;
  while i < args.len() {
    match args[i].as_str() {
      "--input" | "-i" if i + 1 < args.len() => {
        input.url = args[i + 1].clone().trim().to_string();
        i += 1;
      }
      "--keep-alive" | "-a" => {
        input.keep_alive = true;
      }
      _ => {}
    }
    i += 1;
  }

  input
}

fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(
    move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.url.contains("tag=")
        && result.lock().unwrap().is_empty() {
        let mut asd = result.lock().unwrap();
        *asd = event.params.request.url.to_owned();
      }

      RequestPausedDecision::Continue(None)
    },
  )
}

fn download_video(url: &str) -> Result<(), Box<dyn Error>> {
  let mut output_name = url.split('/').last().unwrap().split('.').collect::<Vec<&str>>()[0].to_string();
  output_name.push_str(".mp4");
  let output = Command::new("ffmpeg")
    .arg("-y")
    .args(["-i", &url])
    .args(["-c", "copy"])
    .arg(output_name)
    .output()?;

  if output.status.success() {
    Ok(())
  } else {
    Err(format!("ffmpeg command failed with status: {}", output.status).into())
  }
}
