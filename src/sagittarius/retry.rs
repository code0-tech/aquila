use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::time::{Duration, sleep};
use tonic::transport::{Channel, Endpoint};

const MAX_BACKOFF: u64 = 2000 * 60;
const MAX_RETRIES: i8 = 10;

// Wiöö create a channel and retry if its not possible
pub async fn create_channel_with_retry(
    channel_name: &str,
    url: String,
    ready: Arc<AtomicBool>,
) -> Channel {
    let mut backoff = 100;
    let mut retires = 0;

    loop {
        ready.store(false, Ordering::SeqCst);

        let channel = match Endpoint::from_shared(url.clone()) {
            Ok(c) => {
                log::debug!("Creating a new endpoint for the: {} Service", channel_name);
                c.connect_timeout(Duration::from_secs(2))
                    .timeout(Duration::from_secs(10))
            }
            Err(err) => {
                panic!(
                    "Cannot create Endpoint for Service: `{}`. Reason: {:?}",
                    channel_name, err
                );
            }
        };

        match channel.connect().await {
            Ok(ch) => {
                return ch;
            }
            Err(err) => {
                log::warn!(
                    "Retry connect to `{}` using url: `{}` failed: {:?}, retrying in {}ms",
                    channel_name,
                    url,
                    err,
                    backoff
                );
                sleep(Duration::from_millis(backoff)).await;

                backoff = (backoff * 2).min(MAX_BACKOFF);
                retires += 1;

                if retires >= MAX_RETRIES {
                    panic!("Reached max retries to url {}", url)
                }
            }
        }
    }
}
