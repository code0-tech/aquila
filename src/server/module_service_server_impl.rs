use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    configuration::service::ServiceConfiguration,
    sagittarius::module_service_client_impl::SagittariusModuleServiceClient,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::module_service_server::ModuleService;

pub struct AquilaModuleServiceServer {
    service_configuration: ServiceConfiguration,
    client: Arc<Mutex<SagittariusModuleServiceClient>>,
}

impl AquilaModuleServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusModuleServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl ModuleService for AquilaModuleServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::ModuleUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::ModuleUpdateResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => {
                log::warn!("Rejected module update reason=missing_or_invalid_token");
                return Err(status);
            }
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            log::warn!(
                "Rejected module update reason=token_not_registered token={}",
                token
            );
            return Err(Status::unauthenticated("token is not valid"));
        }

        let modules_update_request = request.into_inner();

        log::debug!(
            "Received module update modules={:?}",
            modules_update_request
                .modules
                .iter()
                .map(|d| d.identifier.clone())
                .collect::<Vec<_>>()
        );

        let mut client = self.client.lock().await;
        let response = client.update_modules(modules_update_request).await;

        log::debug!("Completed module update success={}", response.success);

        Ok(tonic::Response::new(tucana::aquila::ModuleUpdateResponse {
            success: response.success,
        }))
    }
}
