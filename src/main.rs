use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::{Arc, Mutex},
  time::Duration,
};

use reqwest::Client;

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
use tokio::process::Command;

struct InputArgs {
  url: String,
  keep_alive: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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

  let client = Arc::new(Client::new());

  if !input.url.is_empty() {
    let browser_clone = browser.clone();
    let client_clone = client.clone();
    match download_video(&browser_clone, &client_clone, &input.url).await {
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

    let browser_clone = browser.clone();
    let client_clone = client.clone();
    tokio::spawn(async move {
      let id = tokio::task::id();
      match download_video(&browser_clone, &client_clone, &input_url).await {
        Ok(_) => {
          println!("Task {}: Successfully downloaded video", id);
        }
        Err(e) => {
          println!("Task {}: Failed to download video: {}", id, e);
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

async fn get_media_playlist_urls(
  client: &Client,
  master_playlist_url: &str,
) -> Result<(String, String), Box<dyn Error>> {
  let twimg = String::from("https://video.twimg.com");
  let (mut video, mut audio) = (twimg.clone(), twimg.clone());

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

async fn get_segment_urls(text: &str) -> Vec<String> {
  text
    .lines()
    .filter(|line| line.contains("/ext_tw_video/"))
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

async fn download_segments(client: &Client, urls: (String, String)) -> Result<(Vec<u8>, Vec<u8>), Box<dyn Error>> {
  let video_text;
  let audio_text;

  match client.get(urls.0).send().await {
    Ok(response) => {
      video_text = response.text().await.unwrap();
    }
    Err(_) => return Err("Failed to get video segments".into()),
  }
  match client.get(urls.1).send().await {
    Ok(response) => {
      audio_text = response.text().await.unwrap();
    }
    Err(_) => return Err("Failed to get audio segments".into()),
  }

  let video_urls = get_segment_urls(&video_text).await;
  let audio_urls = get_segment_urls(&audio_text).await;

  let video_data = Arc::new(Mutex::new({
    let mut v = Vec::with_capacity(video_urls.len());
    v.extend((0..video_urls.len()).map(|_| Vec::new()));
    v
  }));
  
  let audio_data = Arc::new(Mutex::new({
    let mut v = Vec::with_capacity(audio_urls.len());
    v.extend((0..audio_urls.len()).map(|_| Vec::new()));
    v
  }));

  let video_task = {
    let client = client.clone();
    let video_data = video_data.clone();
    tokio::spawn(async move {
      for (i, url) in video_urls.into_iter().enumerate() {
        let response = client.get(url).send().await.unwrap();
        let data = response.bytes().await.unwrap().to_vec();
        video_data.lock().unwrap()[i] = data;
      }
    })
  };

  if video_task.await.is_err() {
    return Err("Failed to download video segments".into());
  }

  let audio_task = {
    let client = client.clone();
    let audio_data = audio_data.clone();
    tokio::spawn(async move {
      for (i, url) in audio_urls.into_iter().enumerate() {
        let response = client.get(url).send().await.unwrap();
        let data = response.bytes().await.unwrap().to_vec();
        audio_data.lock().unwrap()[i] = data;
      }
    })
  };

  if audio_task.await.is_err() {
    return Err("Failed to download audio segments".into());
  }

  let video_data = Arc::try_unwrap(video_data).unwrap().into_inner().unwrap().concat();
  let audio_data = Arc::try_unwrap(audio_data).unwrap().into_inner().unwrap().concat();
  Ok((video_data, audio_data))
}

async fn download_video(browser: &Browser, client: &Client, url: &str) -> Result<(), Box<dyn Error>> {
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

  let mut media_urls = (String::new(), String::new());
  match get_media_playlist_urls(client, &master_playlist_url).await {
    Ok(result) => {
      media_urls.0.push_str(&result.0);
      media_urls.1.push_str(&result.1);
    }
    Err(_) => {
      return Err("Failed to close the tab".into());
    }
  }
  let id = tokio::task::id();
  let output_name = media_urls.0.split('/').last().unwrap().replace(".m3u8", ".mp4");
  let video_name = format!("video_{}_{}", id, output_name);
  let audio_name = format!("audio_{}_{}", id, output_name);
  
  let segments = match download_segments(client, media_urls.clone()).await {
    Ok(segments) => segments,
    Err(_) => {
      return Err(format!("Failed to download segments").into());
    }
  };
  tokio::fs::write(video_name.clone(), segments.0).await?;
  tokio::fs::write(audio_name.clone(), segments.1).await?;

  let output = Command::new("ffmpeg")
    .args(&["-i", &video_name])
    .args(&["-i", &audio_name])
    .args(["-c", "copy"])
    .arg("-y")
    .arg(&output_name)
    .output().await;

  std::fs::remove_file(video_name)?;
  std::fs::remove_file(audio_name)?;
  
  if output.is_err() || !output.unwrap().status.success() {
    return Err("Failed to merge video and audio segments".into());
  }

  Ok(())
}
