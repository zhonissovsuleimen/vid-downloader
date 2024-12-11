use std::{
  env::args,
  error::Error,
  io::{self, Write},
};

use downloader::Downloader;

mod downloader;
mod downloader_error;

struct InputArgs {
  url: String,
  keep_alive: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let downloader = Downloader::new();

  const USAGE: &str = "Usage: vid-downloader [options]\n\
    Options:\n  -i --input: input url\n\
      -a --keep-alive: keep handling incoming links (type exit to quit)\n\
    ";

  let args: Vec<String> = args().collect();
  let input = parse_input(args);

  if input.url.is_empty() && !input.keep_alive {
    println!("{}", USAGE);
    return Ok(());
  }
  
  if !input.url.is_empty() {
    // match test.down;
    match downloader.download(&input.url).await {
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

    match downloader.download(&input.url).await {
      Ok(_) => {
        println!("Successfully downloaded video");
      }
      Err(e) => {
        println!("Failed to download video: {}", e);
      }
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