//! Shared connection logic for every long-lived Sagittarius client: dial
//! with exponential backoff, and flip a shared readiness flag as the
//! connection comes up or drops so [`crate::server::create_readiness_interceptor`]
//! can reject gRPC traffic while Sagittarius is unreachable.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::time::{Duration, sleep};
use tonic::transport::{Channel, Endpoint};

const MAX_BACKOFF: u64 = 2000 * 60;
const MAX_RETRIES: i8 = 10;

/// Connects to `url`, retrying with exponential backoff (capped at
/// [`MAX_BACKOFF`] ms) up to [`MAX_RETRIES`] times before giving up.
/// `ready` is cleared before each attempt and set once connected, so callers
/// can gate readiness on it without polling the channel state directly.
///
/// Panics rather than returning an error on exhaustion or a malformed URL,
/// since none of Aquila's callers have a meaningful way to run without a
/// Sagittarius connection.
pub async fn create_channel_with_retry(
    channel_name: &str,
    url: String,
    ready: Arc<AtomicBool>,
) -> Channel {
    let mut backoff = 100;
    let mut retries = 0;

    loop {
        ready.store(false, Ordering::SeqCst);
        let attempt = retries + 1;
        log::debug!(
            "Dialing Sagittarius channel={} url={} attempt={}",
            channel_name,
            url,
            attempt
        );

        let channel = match Endpoint::from_shared(url.clone()) {
            Ok(c) => {
                log::debug!(
                    "Created Sagittarius endpoint channel={} url={}",
                    channel_name,
                    url
                );
                c.connect_timeout(Duration::from_secs(2))
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
                ready.store(true, Ordering::SeqCst);
                log::info!(
                    "Successfully connected channel={} url={} attempt={}",
                    channel_name,
                    url,
                    attempt
                );
                return ch;
            }
            Err(err) => {
                log::warn!(
                    "Connection failed channel={} url={} attempt={} error={:?} retry_in_ms={}",
                    channel_name,
                    url,
                    attempt,
                    err,
                    backoff
                );
                sleep(Duration::from_millis(backoff)).await;

                backoff = (backoff * 2).min(MAX_BACKOFF);
                retries += 1;

                if retries >= MAX_RETRIES {
                    panic!(
                        "Sagittarius connection retries exhausted channel={} url={} attempts={} last_error={:?}",
                        channel_name, url, MAX_RETRIES, err
                    );
                }
            }
        }
    }
}
