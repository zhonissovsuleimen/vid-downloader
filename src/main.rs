use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::{Arc, Mutex},
  time::Duration,
};

use tokio::process::Command;

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

#[tokio::main]
async fn main() {
  const USAGE: &str = "Usage: vid-downloader [options]\nOptions:\n  -i --input: input url\n  -a --keep-alive: keep handling incoming links\n";
  let args: Vec<String> = args().collect();
  let input = parse_input(args);

  if input.url.is_empty() && !input.keep_alive {
    println!("{}", USAGE);
    return;
  }

  println!("Starting browser");
  let browser = match get_browser() {
    Ok(browser) => browser,
    Err(e) => {
      eprintln!("Failed to start browser: {}", e);
      return;
    }
  };
  let browser = Arc::new(browser);

  let caching_clone = browser.clone();
  tokio::spawn(async move {
    cache_twitter(&caching_clone).await;
  });

  loop {
    let mut url = String::new();
    if input.url.is_empty() {
      io::stdout().flush().unwrap();
      if io::stdin().read_line(&mut url).is_err() {
        eprintln!("Failed to read line");
        continue;
      }
    }

    let browser_clone = browser.clone();
    tokio::spawn(async move {
      download_video(&browser_clone, url.trim()).await;
    });

    if !input.keep_alive {
      break;
    }
  }
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

fn get_browser() -> Result<Browser, Box<dyn Error>> {
  Ok(Browser::new(LaunchOptions {
    idle_browser_timeout: Duration::from_secs(1e7 as u64),
    args: vec![std::ffi::OsStr::new("--incognito")],
    ..Default::default()
  })?)
}

fn get_twitter_pattern() -> RequestPattern {
  RequestPattern {
    url_pattern: Some("https://video.twimg.com/*_video/*".to_string()),
    resource_Type: Some(ResourceType::Xhr),
    request_stage: Some(RequestStage::Request),
  }
}

fn get_interceptor(result: Arc<Mutex<String>>) -> Arc<dyn RequestInterceptor + Send + Sync> {
  Arc::new(
    move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.url.contains("tag=") && result.lock().unwrap().is_empty() {
        let mut asd = result.lock().unwrap();
        *asd = event.params.request.url.to_owned();
      }

      RequestPausedDecision::Continue(None)
    },
  )
}

async fn cache_twitter(browser: &Browser) {
  let id = tokio::task::id();

  let tab = match browser.new_tab() {
    Ok(tab) => tab,
    Err(e) => {
      eprintln!("Failed cache twitter: {}", e);
      return;
    }
  };

  let pattern = get_twitter_pattern();
  match tab.enable_fetch(Some(&vec![pattern]), None) {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed cache twitter: {}", e);
      return;
    }
  }

  let intercepted_result = Arc::new(Mutex::new(String::new()));
  let interceptor = get_interceptor(intercepted_result.clone());
  match tab.enable_request_interception(interceptor) {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed cache twitter: {}", e);
      return;
    }
  }

  match tab.navigate_to("https://x.com/jack/status/20") {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed cache twitter: {}", e);
      return;
    }
  }

  match tab.wait_until_navigated() {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed cache twitter: {}", e);
      return;
    }
  }

  println!("Task {}: Successfully cached twitter", id);
  let _ = tab.close(false);
}

async fn execute_ffmpeg(url: &str) -> Result<(), Box<dyn Error>> {
  let mut output_name = url.split('/').last().unwrap().split('.').collect::<Vec<&str>>()[0].to_string();
  output_name.push_str(".mp4");
  let output = Command::new("ffmpeg")
    .arg("-y")
    .args(["-i", &url])
    .args(["-c", "copy"])
    .arg(output_name)
    .output()
    .await;

  match output {
    Ok(output) if output.status.success() => Ok(()),
    _ => Err("Failed to execute ffmpeg command".into()),
  }
}

async fn download_video(browser: &Browser, url: &str) {
  let id = tokio::task::id();

  let tab = match browser.new_tab() {
    Ok(tab) => {
      println!("Task {}: Successfully opened tab", id);
      tab
    }
    Err(e) => {
      eprintln!("Failed to open tab: {}", e);
      return;
    }
  };

  let pattern = get_twitter_pattern();
  match tab.enable_fetch(Some(&vec![pattern]), None) {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed to enable fetch: {}", e);
      return;
    }
  }

  let intercepted_result = Arc::new(Mutex::new(String::new()));
  let interceptor = get_interceptor(intercepted_result.clone());
  match tab.enable_request_interception(interceptor) {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed to enable request interception: {}", e);
      return;
    }
  }

  match tab.navigate_to(url) {
    Ok(_) => {}
    Err(e) => {
      eprintln!("Failed to navigate to link: {}", e);
      return;
    }
  }

  match tab.wait_until_navigated() {
    Ok(_) => {
      println!("Task {}: Successfully navigated to link", id);
    }
    Err(e) => {
      eprintln!("Failed to navigate to link: {}", e);
      return;
    }
  }

  let m3u8_url = intercepted_result.lock().unwrap().to_owned();
  match m3u8_url.is_empty() {
    true => {
      eprintln!("Failed to find m3u8 url");
      return;
    }
    false => {
      println!("Task {}: Found m3u8 url: {}", id, m3u8_url);
    }
  }

  match execute_ffmpeg(&m3u8_url).await {
    Ok(_) => {
      println!("Task {}: Successfully downloaded video", id);
    }
    Err(e) => {
      eprintln!("Failed to download video: {}", e);
    }
  }

  match tab.close(false) {
    Ok(_) => {
      println!("Task {}: Successfully closed tab", id);
    }
    Err(e) => {
      eprintln!("Failed to close tab: {}", e);
    }
  }
}
