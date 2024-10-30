use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use headless_chrome::browser::tab::{RequestInterceptor, RequestPausedDecision};
use headless_chrome::browser::transport::{SessionId, Transport};
use headless_chrome::protocol::cdp::Fetch::events::RequestPausedEvent;
use headless_chrome::{Browser, LaunchOptions};

fn main() -> Result<(), Box<dyn Error>> {
  let test_link = r"https://x.com/shitpost_2077/status/1851260612161966480";
  let result = Arc::new(Mutex::new(String::new()));
  let result_clone = result.clone();

  let browser = Browser::new( LaunchOptions {
    idle_browser_timeout: Duration::from_secs(60),
    ..Default::default()
  })?;

  let tab = browser.new_tab()?;

  let interceptor: Arc<dyn RequestInterceptor + Send + Sync> = Arc::new(
    move |_transport: Arc<Transport>, _session_id: SessionId, event: RequestPausedEvent| {
      let request = event.params.request.clone();

      if request.method == "GET" 
        && request.url.starts_with("https://video.twimg.com/ext_tw_video/")
        && request.url.contains("tag=12") {
        let mut asd = result_clone.lock().unwrap();
        *asd = event.params.request.url.to_owned();
      }

      RequestPausedDecision::Continue(None)
    },
  );

  tab.enable_fetch(None, None)?;
  tab.enable_request_interception(interceptor)?;

  tab.navigate_to(&test_link)?.wait_until_navigated()?;

  println!("{:?}", result.lock().unwrap());

  Ok(())
}
