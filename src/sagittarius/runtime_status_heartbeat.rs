//! Background job that sends Aquila's own heartbeat to the Sagittarius
//! gateway on a configurable schedule (`runtime_status.heartbeat_interval_minutes`),
//! so Sagittarius can tell Aquila itself apart from a silent runtime.

use std::{sync::Arc, time::Duration};

use tokio::sync::Mutex;

use super::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient;

/// Spawns the periodic heartbeat. Runs until the process exits; there is no
/// cancellation handle since the caller supervises it via the returned
/// `JoinHandle` instead.
pub fn spawn(
    client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
    heartbeat_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(heartbeat_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let mut client = client.lock().await;
            client.send_heartbeat().await;
        }
    })
}
