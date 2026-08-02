//! Background task that periodically checks tracked runtimes for timeouts
//! and would forward the resulting status transitions to Sagittarius.
//!
//! Forwarding is currently disabled pending #360 — the tick still runs and
//! computes the updates so the logic stays exercised, it just doesn't send
//! them anywhere yet.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

use crate::sagittarius::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient;

use super::registry::TrackedRuntimeRegistry;

/// Spawns the periodic timeout check. Runs until the process exits; there is
/// no cancellation handle because the server itself never tears this down.
pub(super) fn spawn(
    registry: TrackedRuntimeRegistry,
    _client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
    not_responding_after: Duration,
    stopped_after_not_responding: Duration,
    monitor_interval: Duration,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(monitor_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            let timeout_updates = registry
                .collect_timeout_updates(Instant::now(), not_responding_after, stopped_after_not_responding)
                .await;

            if timeout_updates.is_empty() {
                continue;
            }

            let mut client = _client.lock().await;
            for timeout_update in timeout_updates {
                let _ = client.update_runtime_status(timeout_update).await;
            }
        }
    });
}
