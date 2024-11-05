use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::{Arc, Mutex},
  time::Duration,
};

use futures::future;
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
use reqwest;
use tokio::{process::Command, signal};

struct InputArgs {
  url: String,
  keep_alive: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  const USAGE: &str =
    "Usage: vid-downloader [options]\nOptions:\n  -i --input: input url\n  -a --keep-alive: keep handling incoming links\n";
  let args: Vec<String> = args().collect();
  let input = parse_input(args);

  if input.url.is_empty() && !input.keep_alive {
    println!("{}", USAGE);
    return Ok(());
  }

  let browser = Arc::new(get_browser()?);
  let browser_process_id = browser.get_process_id();
  tokio::spawn(async move {
    let _ = signal::ctrl_c().await;
    if let Some(pid) = browser_process_id {
      let _ = Command::new("taskkill").args(&["/F", "/PID", &pid.to_string()]).output().await;
    }
  });
  
  if !input.url.is_empty() {
    let browser_clone = browser.clone();
    match download_video(&browser_clone, &input.url).await {
      Ok(_) => {
        println!("Successfully downloaded video");
      }
      Err(e) => {
        println!("Failed to download video: {}", e);
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
    if input_url.trim().to_lowercase() == "exit" {
      break;
    }

    let browser_clone = browser.clone();
    tokio::spawn(async move {
      let id = tokio::task::id();
      match download_video(&browser_clone, &input_url).await {
        Ok(_) => {
          println!("Task {}: Successfully downloaded video", id);
        }
        Err(e) => {
          println!("Task {}: Failed to download video: {}", id, e);
        }
      }
    });
  }

  if let Some(pid) = browser_process_id {
    let _ = Command::new("taskkill").args(&["/F", "/PID", &pid.to_string()]).output().await?;
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
        let mut mutex_guard = result.lock().unwrap();
        let pure_url = match request.url.find('?') {
          Some(index) => request.url[..index].to_string(),
          None => request.url,
        };

        *mutex_guard = pure_url;
      }

      RequestPausedDecision::Continue(None)
    },
  )
}

async fn get_media_playlist_urls(master_playlist_url: &str) -> Result<(String, String), Box<dyn Error>> {
  let response = reqwest::get(master_playlist_url).await;
  let lines: Vec<String>;
  match response {
    Ok(result) => {
      lines = result
        .text()
        .await
        .unwrap()
        .lines()
        .filter(|line| (line.contains("/ext_tw_video/") || line.contains("/amplify_video/")) && !line.contains("TYPE=SUBTITLES"))
        .map(|line| line.to_string())
        .collect();
    }
    Err(_) => {
      return Err("Failed to fetch master playlist".into());
    }
  }

  let twimg = String::from("https://video.twimg.com");
  let (mut video, mut audio) = (twimg.clone(), twimg.clone());
  match lines.len() > 1 {
    true => {
      let pure_audio = lines[0].split('"').filter(|substring| !substring.is_empty()).last().unwrap();
      audio.push_str(pure_audio);
      video.push_str(lines[lines.len() / 2].as_str());

      return Ok((video, audio));
    }
    false => {
      return Err("Failed to extract video and audio m3u8 links".into());
    }
  }
}

async fn get_segment_urls(text: &str) -> Vec<String> {
  text
    .lines()
    .filter(|line| line.contains("/ext_tw_video/") || line.contains("/amplify_video/"))
    .map(|line| {
      let split = line.split('"').filter(|substr| !substr.is_empty());
      let mut result = String::from("https://video.twimg.com");
      if let Some(url) = split.last() {
        result.push_str(url);
      }
      result
    })
    .collect::<Vec<String>>()
}

fn get_download_tasks(urls: Vec<String>, data: Arc<Mutex<Vec<Vec<u8>>>>) -> Vec<tokio::task::JoinHandle<()>> {
  let mut tasks = Vec::new();
  for (i, url) in urls.into_iter().enumerate() {
    let data = data.clone();
    let task = tokio::spawn(async move {
      let response = reqwest::get(url).await.unwrap();
      let bytes = response.bytes().await.unwrap().to_vec();
      data.lock().unwrap()[i] = bytes;
    });
    tasks.push(task);
  }

  tasks
}

async fn download_segments(urls: (String, String)) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
  let video_text = reqwest::get(urls.0).await?.text().await?;
  let audio_text = reqwest::get(urls.1).await?.text().await?;

  let video_urls = get_segment_urls(&video_text).await;
  let audio_urls = get_segment_urls(&audio_text).await;

  let video_data = Arc::new(Mutex::new(vec![Vec::new(); video_urls.len()]));
  let audio_data = Arc::new(Mutex::new(vec![Vec::new(); audio_urls.len()]));

  let video_tasks = get_download_tasks(video_urls, video_data.clone());
  let audio_tasks = get_download_tasks(audio_urls, audio_data.clone());

  let all_tasks: Vec<_> = video_tasks.into_iter().chain(audio_tasks.into_iter()).collect();

  future::join_all(all_tasks).await;

  let video_data = Arc::try_unwrap(video_data).unwrap().into_inner().unwrap().concat();
  let audio_data = Arc::try_unwrap(audio_data).unwrap().into_inner().unwrap().concat();
  Ok((video_data, audio_data))
}

async fn download_video(browser: &Browser, url: &str) -> Result<(), Box<dyn Error>> {
  let target = CreateTarget {
    url: "about::blank".to_string(),
    width: None,
    height: None,
    browser_context_id: None,
    enable_begin_frame_control: None,
    new_window: Some(true),
    background: Some(true),
  };

  let tab = browser.new_tab_with_options(target)?;
  let intercepted_result = Arc::new(Mutex::new(String::new()));
  let interceptor = get_interceptor(intercepted_result.clone());

  let pattern = get_twitter_pattern();
  tab.enable_fetch(Some(&vec![pattern]), None)?;
  tab.enable_request_interception(interceptor)?;

  tab.navigate_to(url)?;
  let mut found = false;
  while !found {
    found = !intercepted_result.lock().unwrap().is_empty();
  }

  let master_playlist_url = intercepted_result.lock().unwrap().to_owned();
  if master_playlist_url.is_empty() {
    return Err("Failed to find the m3u8 url".into());
  }
  let _ = tab.close(false);

  let media_urls = get_media_playlist_urls(&master_playlist_url).await?;

  let id = std::process::id();
  let output_name = media_urls.0.split('/').last().unwrap().replace(".m3u8", ".mp4");
  let video_name = format!("video_{}_{}", id, output_name);
  let audio_name = format!("audio_{}_{}", id, output_name);

  let segments = download_segments(media_urls.clone()).await?;

  tokio::fs::write(video_name.clone(), segments.0).await?;
  tokio::fs::write(audio_name.clone(), segments.1).await?;

  let output = Command::new("ffmpeg")
    .args(&["-i", &video_name])
    .args(&["-i", &audio_name])
    .args(["-c", "copy"])
    .arg("-y")
    .arg(&output_name)
    .output()
    .await;

  std::fs::remove_file(video_name)?;
  std::fs::remove_file(audio_name)?;

  if output.is_err() || !output.unwrap().status.success() {
    return Err("Failed to merge video and audio segments".into());
  }

  Ok(())
}
