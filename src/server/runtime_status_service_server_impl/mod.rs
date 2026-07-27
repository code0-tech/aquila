//! gRPC server for runtime heartbeats (`RuntimeStatusService.Update`).
//!
//! Accepting a heartbeat and detecting a runtime's silence are two
//! different lifetimes: the RPC handler below only needs to authenticate
//! and record the heartbeat, while a background tick (spawned once at
//! construction, see [`monitor`]) is what actually notices when a runtime
//! stops sending them. Both share the same [`registry::TrackedRuntimeRegistry`].

mod monitor;
mod registry;

use std::{sync::Arc, time::Duration};

use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::runtime_status_service_server::RuntimeStatusService;

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient,
};

use registry::TrackedRuntimeRegistry;

pub struct AquilaRuntimeStatusServiceServer {
    client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
    service_configuration: ServiceConfiguration,
    tracked_runtimes: TrackedRuntimeRegistry,
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
            tracked_runtimes: TrackedRuntimeRegistry::default(),
            not_responding_after,
            stopped_after_not_responding,
        };

        monitor::spawn(
            server.tracked_runtimes.clone(),
            server.client.clone(),
            server.not_responding_after,
            server.stopped_after_not_responding,
            monitor_interval,
        );
        server
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
        self.tracked_runtimes
            .record_heartbeat(&runtime_status_update_request)
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
