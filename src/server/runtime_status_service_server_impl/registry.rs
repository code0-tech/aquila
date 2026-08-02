//! Tracks the last known status of every runtime that has sent a heartbeat,
//! and derives NOT_RESPONDING / STOPPED transitions from how long a runtime
//! has gone silent. This is pure bookkeeping — actually forwarding those
//! transitions to Sagittarius is [`super::monitor`]'s job.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use tokio::sync::Mutex;
use tucana::{aquila::RuntimeStatusUpdateRequest, shared::ModuleStatus, shared::module_status};

#[derive(Clone)]
pub(super) struct RuntimeStatusSnapshot {
    status: ModuleStatus,
}

impl RuntimeStatusSnapshot {
    fn from_update(request: &RuntimeStatusUpdateRequest) -> Option<Self> {
        let status = request.status.as_ref()?;

        if status.identifier.is_empty() {
            None
        } else {
            Some(Self {
                status: status.clone(),
            })
        }
    }

    fn key(&self) -> String {
        self.status.identifier.clone()
    }

    pub(super) fn identifier(&self) -> &str {
        &self.status.identifier
    }

    fn is_stopped(&self) -> bool {
        self.status.status == module_status::StatusVariant::Stopped as i32
    }

    fn not_responding_update(&self) -> RuntimeStatusUpdateRequest {
        let mut next_status = self.status.clone();
        next_status.status = module_status::StatusVariant::NotResponding as i32;
        next_status.timestamp = epoch_seconds_now();

        RuntimeStatusUpdateRequest {
            status: Some(next_status),
        }
    }

    fn stopped_update(&self) -> RuntimeStatusUpdateRequest {
        let mut next_status = self.status.clone();
        next_status.status = module_status::StatusVariant::Stopped as i32;
        next_status.timestamp = epoch_seconds_now();

        RuntimeStatusUpdateRequest {
            status: Some(next_status),
        }
    }
}

struct TrackedRuntime {
    last_seen: Instant,
    last_status: RuntimeStatusSnapshot,
    /// `Some` once a timeout tick has flagged this runtime as NOT_RESPONDING;
    /// cleared the moment another heartbeat arrives.
    not_responding_since: Option<Instant>,
}

/// Shared, lock-protected map of runtime identifier to its last known status.
#[derive(Clone, Default)]
pub(super) struct TrackedRuntimeRegistry {
    tracked: Arc<Mutex<HashMap<String, TrackedRuntime>>>,
}

impl TrackedRuntimeRegistry {
    /// Records a heartbeat, or drops tracking entirely if the runtime
    /// reports itself STOPPED — a stopped runtime should not later be
    /// timed out as if it had gone silent.
    pub(super) async fn record_heartbeat(&self, request: &RuntimeStatusUpdateRequest) {
        let Some(snapshot) = RuntimeStatusSnapshot::from_update(request) else {
            log::debug!(
                "Skipping runtime heartbeat tracking because status payload is missing or identifier is empty."
            );
            return;
        };

        let key = snapshot.key();
        let now = Instant::now();
        let mut tracked = self.tracked.lock().await;

        if snapshot.is_stopped() {
            tracked.remove(&key);
            return;
        }

        match tracked.get_mut(&key) {
            Some(runtime) => {
                if runtime.not_responding_since.is_some() {
                    log::info!(
                        "Runtime '{}' sent heartbeat again. Clearing NOT_RESPONDING flag.",
                        snapshot.identifier()
                    );
                }
                runtime.last_seen = now;
                runtime.last_status = snapshot;
                runtime.not_responding_since = None;
            }
            None => {
                tracked.insert(
                    key,
                    TrackedRuntime {
                        last_seen: now,
                        last_status: snapshot,
                        not_responding_since: None,
                    },
                );
            }
        }
    }

    /// Walks every tracked runtime and returns the status updates implied by
    /// how long each has gone silent, advancing (and, for STOPPED, removing)
    /// their tracked state in the process.
    pub(super) async fn collect_timeout_updates(
        &self,
        now: Instant,
        not_responding_after: Duration,
        stopped_after_not_responding: Duration,
    ) -> Vec<RuntimeStatusUpdateRequest> {
        let mut tracked = self.tracked.lock().await;
        collect_timeout_updates(
            &mut tracked,
            now,
            not_responding_after,
            stopped_after_not_responding,
        )
    }
}

fn collect_timeout_updates(
    tracked: &mut HashMap<String, TrackedRuntime>,
    now: Instant,
    not_responding_after: Duration,
    stopped_after_not_responding: Duration,
) -> Vec<RuntimeStatusUpdateRequest> {
    let mut timeout_updates = Vec::new();
    let mut runtimes_to_remove = Vec::new();

    for (key, runtime) in tracked.iter_mut() {
        let silence = now.duration_since(runtime.last_seen);

        match runtime.not_responding_since {
            None if silence >= not_responding_after => {
                log::warn!(
                    "Runtime '{}' has not sent status for {:?}. Marking as NOT_RESPONDING.",
                    runtime.last_status.identifier(),
                    silence
                );
                runtime.not_responding_since = Some(now);
                timeout_updates.push(runtime.last_status.not_responding_update());
            }
            Some(since) if now.duration_since(since) >= stopped_after_not_responding => {
                log::warn!(
                    "Runtime '{}' stayed NOT_RESPONDING for {:?}. Marking as STOPPED.",
                    runtime.last_status.identifier(),
                    now.duration_since(since)
                );
                timeout_updates.push(runtime.last_status.stopped_update());
                runtimes_to_remove.push(key.clone());
            }
            _ => {}
        }
    }

    for key in runtimes_to_remove {
        tracked.remove(&key);
    }

    timeout_updates
}

fn epoch_seconds_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => {
            log::warn!("System time before UNIX_EPOCH: {:?}", error);
            0
        }
    }
}
