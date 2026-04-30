use crate::authorization::authorization::get_authorization_metadata;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius::runtime_status_service_client::RuntimeStatusServiceClient;

pub struct SagittariusRuntimeStatusServiceClient {
    client: RuntimeStatusServiceClient<Channel>,
    token: String,
}

impl SagittariusRuntimeStatusServiceClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = RuntimeStatusServiceClient::new(channel);
        Self { client, token }
    }

    pub async fn update_runtime_status(
        &mut self,
        runtime_status_request: tucana::aquila::RuntimeStatusUpdateRequest,
    ) -> tucana::aquila::RuntimeStatusUpdateResponse {
        log::debug!("Forwarding runtime status update to Sagittarius");
        let status: Option<tucana::sagittarius::runtime_status_update_request::Status> = match runtime_status_request.status {
            Some(stat) => match stat {
                tucana::aquila::runtime_status_update_request::Status::AdapterRuntimeStatus(adapter_runtime_status) => {
                    Some(tucana::sagittarius::runtime_status_update_request::Status::AdapterRuntimeStatus(adapter_runtime_status))
                },
                tucana::aquila::runtime_status_update_request::Status::ExecutionRuntimeStatus(execution_runtime_status) => {
                    Some(tucana::sagittarius::runtime_status_update_request::Status::ExecutionRuntimeStatus(execution_runtime_status))
                },
                tucana::aquila::runtime_status_update_request::Status::ActionStatus(action_status) => {
                    Some(tucana::sagittarius::runtime_status_update_request::Status::ActionStatus(action_status))
                },
            },
            None => None,
        };
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::RuntimeStatusUpdateRequest { status },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Successfully transferred RuntimeStatus.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update RuntimeStatus: {:?}", err);
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
