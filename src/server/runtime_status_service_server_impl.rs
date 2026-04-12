use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration, sagittarius::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::runtime_status_service_server::RuntimeStatusService;

pub struct AquilaRuntimeStatusServiceServer {
    client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
    service_configuration: ServiceConfiguration,
}

impl AquilaRuntimeStatusServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl RuntimeStatusService for AquilaRuntimeStatusServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeStatusUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::RuntimeStatusUpdateResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => return Err(status),
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            return Err(Status::unauthenticated("token is not valid"));
        }

        let runtime_status_update_request = request.into_inner();

        log::debug!(
            "Received Runtime Status Update: {:?}",
            runtime_status_update_request
        );

        let mut client = self.client.lock().await;
        let response = client
            .update_runtime_status(runtime_status_update_request)
            .await;

        Ok(tonic::Response::new(
            tucana::aquila::RuntimeStatusUpdateResponse {
                success: response.success,
            },
        ))
    }
}
