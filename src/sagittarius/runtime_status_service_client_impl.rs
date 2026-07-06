use crate::{authorization::authorization::get_authorization_metadata, telemetry::errors};
use std::time::Duration;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius::runtime_status_service_client::RuntimeStatusServiceClient;

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
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::RuntimeStatusUpdateRequest {
                status: runtime_status_request.status,
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
}
