use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::runtime_function_definition_service_server::RuntimeFunctionDefinitionService;

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration, sagittarius::runtime_function_service_client_impl::SagittariusRuntimeFunctionServiceClient
};

pub struct AquilaRuntimeFunctionServiceServer {
    client: Arc<Mutex<SagittariusRuntimeFunctionServiceClient>>,
    service_configuration: ServiceConfiguration,
}

impl AquilaRuntimeFunctionServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusRuntimeFunctionServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl RuntimeFunctionDefinitionService for AquilaRuntimeFunctionServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeFunctionDefinitionUpdateRequest>,
    ) -> Result<
        tonic::Response<tucana::aquila::RuntimeFunctionDefinitionUpdateResponse>,
        tonic::Status,
    > {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => return Err(status),
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            return Err(Status::unauthenticated("token is not valid"));
        }

        let runtime_function_definition_update_request = request.into_inner();

        log::debug!(
            "Received RuntimeFunctions: {:?}",
            runtime_function_definition_update_request
                .runtime_functions
                .iter()
                .map(|f| f.runtime_name.clone())
                .collect::<Vec<_>>()
        );

        let mut client = self.client.lock().await;
        let response = client
            .update_runtime_function_definitions(runtime_function_definition_update_request)
            .await;

        Ok(tonic::Response::new(
            tucana::aquila::RuntimeFunctionDefinitionUpdateResponse {
                success: response.success,
            },
        ))
    }
}
