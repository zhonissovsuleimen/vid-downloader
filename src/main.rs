use downloader::{Downloader, PreferredResolution};
use std::{
  env::args,
  error::Error,
  io::{self, Write},
  sync::Arc,
};
use tracing_subscriber::fmt::format::FmtSpan;

mod downloader;
mod downloader_error;
mod platforms;
mod playlist;

struct InputArgs {
  url: String,
  keep_alive: bool,
  resolution: Option<PreferredResolution>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  tracing_subscriber::fmt()
    .compact()
    .with_ansi(false)
    .with_file(false)
    .with_level(true)
    .with_line_number(false)
    .with_span_events(FmtSpan::FULL)
    .with_target(false)
    .with_thread_ids(true)
    .with_thread_names(false)
    .init();

  let args: Vec<String> = args().collect();
  if !(args.contains(&String::from("-a")) || args.contains(&String::from("-i"))) {
    const USAGE: &str = "Usage: vid-downloader [options]\n\
    Options:\n\
    -i --input: input url\n\
    -a --keep-alive: keep handling incoming links (type exit to quit)\n\
    ";
    println!("{}", USAGE);
    return Ok(());
  }

  let input = parse_input(args);
  let downloader = Arc::new(Downloader::new());
  if !input.keep_alive {
    let downloader_clone = downloader.clone();
    let resolution_clone = input.resolution.clone();
    let _ = tokio::spawn(async move { downloader_clone.download(&input.url, resolution_clone).await }).await;
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
    let resolution_clone = input.resolution.clone();
    tokio::spawn(async move {
      let _ = downloader_clone.download(&new_url, resolution_clone).await;
    });
  }

  Ok(())
}

fn parse_input(args: Vec<String>) -> InputArgs {
  let mut input = InputArgs { url: String::new(), keep_alive: false, resolution: None };

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
      "--high" | "-h" => {
        input.resolution = Some(PreferredResolution::High);
      }
      "--medium" | "-m" => {
        input.resolution = Some(PreferredResolution::Medium);
      }
      "--low" | "-l" => {
        input.resolution = Some(PreferredResolution::Low);
      }
      _ => {}
    }
    i += 1;
  }

  input
}
