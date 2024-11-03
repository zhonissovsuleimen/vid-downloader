use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::{Arc, Mutex},
  time::Duration,
};

use reqwest::Client;
use tokio::process::Command;

use headless_chrome::protocol::cdp::{
  Fetch::{events::RequestPausedEvent, RequestPattern, RequestStage},
  Network::ResourceType,
};
use headless_chrome::{
  browser::{
    tab::{RequestInterceptor, RequestPausedDecision},
    transport::{SessionId, Transport},
  },
  protocol::cdp::Target::CreateTarget,
};
use headless_chrome::{Browser, LaunchOptions};

struct InputArgs {
  url: String,
  keep_alive: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
  const USAGE: &str = "Usage: vid-downloader [options]\nOptions:\n  -i --input: input url\n  -a --keep-alive: keep handling incoming links\n";
  let args: Vec<String> = args().collect();
  let input = parse_input(args);

  if input.url.is_empty() && !input.keep_alive {
    println!("{}", USAGE);
    return Ok(());
  }

  let browser = match get_browser() {
    Ok(browser) => {
      println!("Successfully started browser");
      Arc::new(browser)
    }
    Err(e) => {
      eprintln!("Failed to start browser: {}", e);
      return Ok(());
    }
  };

  if !input.url.is_empty() {
    let browser_clone = browser.clone();
    match download_video(&browser_clone, &input.url).await {
      Ok(_) => {
        println!("Successfully downloaded video");
      }
      Err(_) => {
        println!("Failed to download video");
      }
    }
  }

  while input.keep_alive {
    let mut input_url = String::new();
    io::stdout().flush().unwrap();
    if io::stdin().read_line(&mut input_url).is_err() {
      eprintln!("Failed to read line");
      continue;
    }

    let browser_clone = browser.clone();
    tokio::spawn(async move {
      let id = tokio::task::id();
      match download_video(&browser_clone, &input_url).await {
        Ok(_) => {
          println!("Task {}: Successfully downloaded video", id);
        }
        Err(_) => {
          println!("Task {}: Failed to download video", id);
        }
      }
    });
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

fn get_browser() -> Result<Browser, Box<dyn Error + Send + Sync>> {
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

async fn get_media_playlist_urls(master_playlist_url: &str) -> Result<(String, String), Box<dyn Error + Send + Sync>> {
  let twimg = String::from("https://video.twimg.com");
  let (mut video, mut audio) = (twimg.clone(), twimg.clone());

  let client = Client::new();

  let response = client.get(master_playlist_url).send().await;

  let lines: Vec<String>;
  match response {
    Ok(result) => {
      lines = result
        .text()
        .await
        .unwrap()
        .lines()
        .filter(|line| line.contains("/ext_tw_video/"))
        .map(|line| line.to_string())
        .collect();
    }
    Err(_) => {
      return Err("Failed to fetch master playlist".into());
    }
  }

  match lines.len() > 1 {
    true => {
      let pure_audio = lines[0]
        .split('"')
        .filter(|substring| !substring.is_empty())
        .last()
        .unwrap();
      audio.push_str(pure_audio);
      video.push_str(lines[lines.len() / 2].as_str());

      return Ok((video, audio));
    }
    false => {
      return Err("Failed to extract video and audio m3u8 links".into());
    }
  }
}

async fn execute_ffmpeg(urls: (String, String)) -> Result<(), Box<dyn Error + Send + Sync>> {
  let mut output_name = urls.0.split('/').last().unwrap().split('.').collect::<Vec<&str>>()[0].to_string();
  output_name.push_str(".mp4");
  let output = Command::new("ffmpeg")
    .arg("-y")
    .args(["-i", &urls.0])
    .args(["-i", &urls.1])
    .args(["-c", "copy"])
    .arg(output_name)
    .output()
    .await;

  match output {
    Ok(output) if output.status.success() => Ok(()),
    _ => Err("Failed to execute the ffmpeg command".into()),
  }
}

async fn download_video(browser: &Browser, url: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
  let target = CreateTarget {
    url: "about::blank".to_string(),
    width: None,
    height: None,
    browser_context_id: None,
    enable_begin_frame_control: None,
    new_window: Some(true),
    background: Some(true),
  };

  let tab;
  match browser.new_tab_with_options(target) {
    Ok(t) => {
      tab = t;
    }
    Err(_) => {
      return Err("Failed to open the tab".into());
    }
  };

  let pattern = get_twitter_pattern();
  if tab.enable_fetch(Some(&vec![pattern]), None).is_err() {
    return Err("Failed to enable fetch".into());
  }

  let intercepted_result = Arc::new(Mutex::new(String::new()));
  let interceptor = get_interceptor(intercepted_result.clone());
  if tab.enable_request_interception(interceptor).is_err() {
    return Err("Failed to enable request interception".into());
  }

  if tab.navigate_to(url).is_err() {
    return Err("Failed to navigate to the link".into());
  }

  if tab.wait_until_navigated().is_err() {
    return Err("Failed to navigate to the link".into());
  }

  let master_playlist_url = intercepted_result.lock().unwrap().to_owned();
  if master_playlist_url.is_empty() {
    return Err("Failed to find the m3u8 url".into());
  }

  match get_media_playlist_urls(&master_playlist_url).await {
    Ok(result) => {
      if execute_ffmpeg(result).await.is_err() {
        return Err("Failed to download the video".into());
      }
    }
    Err(_) => {
      return Err("Failed to close the tab".into());
    }
  }

  if tab.close(false).is_err() {
    return Err("Failed to close the tab".into());
  }
  Ok(())
}
