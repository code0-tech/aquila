use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::function_definition_service_server::FunctionDefinitionService;

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration, sagittarius::function_service_client_impl::SagittariusFunctionDefinitionServiceClient
};

pub struct AquilaFunctionDefinitionServiceServer {
    client: Arc<Mutex<SagittariusFunctionDefinitionServiceClient>>,
    service_configuration: ServiceConfiguration,
}

impl AquilaFunctionDefinitionServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusFunctionDefinitionServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl FunctionDefinitionService for AquilaFunctionDefinitionServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::FunctionDefinitionUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::FunctionDefinitionUpdateResponse>, tonic::Status>
    {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => return Err(status),
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            return Err(Status::unauthenticated("token is not valid"));
        }

        let function_definition_update_request = request.into_inner();

        log::debug!(
            "Received Functions: {:?}",
            function_definition_update_request
                .functions
                .iter()
                .map(|f| f.runtime_name.clone())
                .collect::<Vec<_>>()
        );

        let mut client = self.client.lock().await;
        let response = client
            .update_function_definitions(function_definition_update_request)
            .await;

        Ok(tonic::Response::new(
            tucana::aquila::FunctionDefinitionUpdateResponse {
                success: response.success,
            },
        ))
    }
}
