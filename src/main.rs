use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::Arc,
};

use downloader::Downloader;
use tokio::sync::Mutex;

mod downloader;
mod downloader_error;

struct InputArgs {
  url: String,
  keep_alive: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let downloader = Arc::new(Mutex::new(Downloader::new()));

  const USAGE: &str = "Usage: vid-downloader [options]\n\
    Options:\n\
    -i --input: input url\n\
    -a --keep-alive: keep handling incoming links (type exit to quit)\n\
    ";

  let args: Vec<String> = args().collect();
  if !(args.contains(&String::from("-a")) || args.contains(&String::from("-i"))) {
    println!("{}", USAGE);
    return Ok(());
  }

  let input = parse_input(args);
  if !input.keep_alive {
    let downloader_clone = downloader.clone();
    let _ = tokio::spawn(async move {
      match downloader_clone.lock().await.download(&input.url).await {
        Ok(_) => {
          println!("Successfully downloaded video");
        }
        Err(e) => {
          println!("Failed to download video: {}", e);
        }
      }
    })
    .await;
  }

  while input.keep_alive {
    let mut new_url = String::new();
    io::stdout().flush().unwrap();
    if io::stdin().read_line(&mut new_url).is_err() {
      eprintln!("Failed to read line");
      continue;
    }
    if new_url.trim().to_lowercase() == "exit" {
      break;
    }

    let downloader_clone = downloader.clone();
    tokio::spawn(async move {
      match downloader_clone.lock().await.download(&new_url).await {
        Ok(_) => {
          println!("Successfully downloaded video");
        }
        Err(e) => {
          println!("Failed to download video: {}", e);
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
