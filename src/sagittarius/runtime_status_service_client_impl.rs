//! Client for reporting runtime status to Sagittarius: forwarding a tracked
//! runtime's heartbeats (currently unused end-to-end — see the disabled
//! forwarding call in `server::runtime_status_service_server_impl`, tracked
//! in #360 — but kept ready for when that's re-enabled), and sending
//! Aquila's own heartbeat (used by `sagittarius::runtime_status_heartbeat`).

use crate::{authorization::authorization::get_authentication_metadata, telemetry::errors};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius_gateway::runtime_status_service_client::RuntimeStatusServiceClient;

pub struct SagittariusRuntimeStatusServiceClient {
    client: RuntimeStatusServiceClient<Channel>,
    token: String,
    unary_rpc_timeout: Duration,
}

impl SagittariusRuntimeStatusServiceClient {
    pub fn new(channel: Channel, token: String, unary_rpc_timeout: Duration) -> Self {
        let client = RuntimeStatusServiceClient::new(channel);
        Self {
            client,
            token,
            unary_rpc_timeout,
        }
    }

    pub async fn update_runtime_status(
        &mut self,
        runtime_status_request: tucana::aquila::RuntimeStatusUpdateRequest,
    ) -> tucana::aquila::RuntimeStatusUpdateResponse {
        log::debug!("Forwarding runtime status update to Sagittarius");
        let mut request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius_gateway::RuntimeStatusUpdateRequest {
                status: runtime_status_request.status.map(
                    tucana::sagittarius_gateway::runtime_status_update_request::Status::ModuleStatus,
                ),
            },
        );
        request.set_timeout(self.unary_rpc_timeout);

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Sagittarius accepted the runtime status update");
                response.into_inner()
            }
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.runtime_status.update",
                    &err,
                    format!(
                        "code={} timeout_ms={}",
                        err.code(),
                        self.unary_rpc_timeout.as_millis()
                    ),
                );
                return tucana::aquila::RuntimeStatusUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated runtime status"),
            false => log::warn!("Sagittarius did not update runtime status"),
        };

        tucana::aquila::RuntimeStatusUpdateResponse {
            success: response.success,
        }
    }

    /// Sends Aquila's own heartbeat to Sagittarius, distinct from forwarding
    /// a tracked runtime's status via [`Self::update_runtime_status`].
    pub async fn send_heartbeat(&mut self) -> bool {
        log::debug!("Sending Aquila heartbeat to Sagittarius");
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius_gateway::RuntimeStatusUpdateRequest {
                status: Some(
                    tucana::sagittarius_gateway::runtime_status_update_request::Status::RuntimeStatus(
                        tucana::shared::RuntimeStatus { timestamp },
                    ),
                ),
            },
        );
        request.set_timeout(self.unary_rpc_timeout);

        let response = match self.client.update(request).await {
            Ok(response) => response.into_inner(),
            Err(err) => {
                errors::record(
                    "dependency",
                    "sagittarius.runtime_status.heartbeat",
                    &err,
                    format!(
                        "code={} timeout_ms={}",
                        err.code(),
                        self.unary_rpc_timeout.as_millis()
                    ),
                );
                return false;
            }
        };

        match response.success {
            true => log::debug!("Sagittarius accepted Aquila heartbeat"),
            false => log::warn!("Sagittarius rejected Aquila heartbeat"),
        };

        response.success
    }
}
