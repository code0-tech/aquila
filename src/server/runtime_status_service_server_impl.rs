use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient,
};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tonic::Status;
use tucana::{
    aquila::{RuntimeStatusUpdateRequest, runtime_status_service_server::RuntimeStatusService},
    shared::{ModuleStatus, module_status},
};

#[derive(Clone)]
struct RuntimeStatusSnapshot {
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

    fn identifier(&self) -> &str {
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
    not_responding_since: Option<Instant>,
}

pub struct AquilaRuntimeStatusServiceServer {
    client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
    service_configuration: ServiceConfiguration,
    tracked_runtimes: Arc<Mutex<HashMap<String, TrackedRuntime>>>,
    not_responding_after: Duration,
    stopped_after_not_responding: Duration,
}

impl AquilaRuntimeStatusServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
        service_configuration: ServiceConfiguration,
        not_responding_after: Duration,
        stopped_after_not_responding: Duration,
        monitor_interval: Duration,
    ) -> Self {
        let server = Self {
            client,
            service_configuration,
            tracked_runtimes: Arc::new(Mutex::new(HashMap::new())),
            not_responding_after,
            stopped_after_not_responding,
        };

        server.spawn_timeout_monitor(monitor_interval);
        server
    }

    fn spawn_timeout_monitor(&self, monitor_interval: Duration) {
        let tracked_runtimes = self.tracked_runtimes.clone();
        let _client = self.client.clone();
        let not_responding_after = self.not_responding_after;
        let stopped_after_not_responding = self.stopped_after_not_responding;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(monitor_interval);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                interval.tick().await;

                let timeout_updates = {
                    let mut tracked = tracked_runtimes.lock().await;
                    collect_timeout_updates(
                        &mut tracked,
                        Instant::now(),
                        not_responding_after,
                        stopped_after_not_responding,
                    )
                };

                if timeout_updates.is_empty() {
                    continue;
                }

                // Uncomment in #360
                // let mut client = _client.lock().await;
                for _timeout_update in timeout_updates {
                    // let _ = client.update_runtime_status(timeout_update).await;
                }
            }
        });
    }

    async fn track_runtime_update(
        &self,
        runtime_status_update_request: &RuntimeStatusUpdateRequest,
    ) {
        let Some(snapshot) = RuntimeStatusSnapshot::from_update(runtime_status_update_request)
        else {
            log::debug!(
                "Skipping runtime heartbeat tracking because status payload is missing or identifier is empty."
            );
            return;
        };

        let key = snapshot.key();
        let now = Instant::now();
        let mut tracked = self.tracked_runtimes.lock().await;

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

#[tonic::async_trait]
impl RuntimeStatusService for AquilaRuntimeStatusServiceServer {
    #[tracing::instrument(
        name = "aquila.runtime_status.update",
        skip_all,
        fields(rpc.system = "grpc", rpc.service = "RuntimeStatusService", rpc.method = "Update")
    )]
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeStatusUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::RuntimeStatusUpdateResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t.to_string(),
            Err(status) => {
                log::warn!("Rejected runtime status update reason=missing_or_invalid_token");
                return Err(status);
            }
        };

        let runtime_status_update_request = request.into_inner();

        let runtime_identifier = match runtime_status_update_request.status.as_ref() {
            Some(status) => status.identifier.clone(),
            None => return Err(Status::invalid_argument("missing runtime status payload")),
        };

        if runtime_identifier.is_empty() {
            return Err(Status::invalid_argument("runtime identifier is missing"));
        }

        if !self
            .service_configuration
            .has_service(&token, &runtime_identifier)
        {
            log::warn!(
                "Rejected runtime status update reason=token_not_registered runtime_identifier={}",
                runtime_identifier
            );
            return Err(Status::unauthenticated("token is not valid"));
        }
        self.track_runtime_update(&runtime_status_update_request)
            .await;

        log::debug!(
            "Received runtime status update runtime_identifier={}",
            runtime_identifier
        );

        // Temporarily disabled: do not forward runtime status updates to Sagittarius.
        // Re-enable with the timeout updates above when runtime usage reporting is restored.
        // let mut client = self.client.lock().await;
        // let response = client
        //     .update_runtime_status(runtime_status_update_request)
        //     .await;
        let response = tucana::aquila::RuntimeStatusUpdateResponse { success: true };

        log::debug!(
            "Completed runtime status update success={}",
            response.success
        );

        Ok(tonic::Response::new(
            tucana::aquila::RuntimeStatusUpdateResponse {
                success: response.success,
            },
        ))
    }
}
