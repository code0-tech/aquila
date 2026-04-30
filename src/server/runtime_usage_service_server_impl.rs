use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::runtime_usage_service_server::RuntimeUsageService;

use crate::{
    authorization::authorization::{extract_token, mask_token},
    configuration::service::ServiceConfiguration,
    sagittarius::runtime_usage_client_impl::SagittariusRuntimeUsageClient,
};

pub struct AquilaRuntimeUsageServiceServer {
    client: Arc<Mutex<SagittariusRuntimeUsageClient>>,
    service_configuration: ServiceConfiguration,
}

impl AquilaRuntimeUsageServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusRuntimeUsageClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl RuntimeUsageService for AquilaRuntimeUsageServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeUsageRequest>,
    ) -> Result<tonic::Response<tucana::aquila::RuntimeUsageResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => {
                log::warn!("Rejected runtime usage update reason=missing_or_invalid_token");
                return Err(status);
            }
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            log::warn!(
                "Rejected runtime usage update reason=token_not_registered token={}",
                mask_token(token)
            );
            return Err(Status::unauthenticated("token is not valid"));
        }

        let runtime_usage_request = request.into_inner();

        log::debug!("Received RuntimeUsageRequest");

        let mut client = self.client.lock().await;
        let response = client.update_runtime_usage(runtime_usage_request).await;
        log::debug!(
            "Completed runtime usage update success={}",
            response.success
        );

        Ok(tonic::Response::new(tucana::aquila::RuntimeUsageResponse {
            success: response.success,
        }))
    }
}
