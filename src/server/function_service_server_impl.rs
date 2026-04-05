use std::sync::Arc;

use tokio::sync::Mutex;
use tucana::aquila::function_definition_service_server::FunctionDefinitionService;

use crate::sagittarius::function_service_client_impl::SagittariusFunctionServiceClient;

pub struct AquilaFunctionServiceServer {
    client: Arc<Mutex<SagittariusFunctionServiceClient>>,
}

impl AquilaFunctionServiceServer {
    pub fn new(client: Arc<Mutex<SagittariusFunctionServiceClient>>) -> Self {
        Self { client }
    }
}

#[tonic::async_trait]
impl FunctionDefinitionService for AquilaFunctionServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::FunctionDefinitionUpdateRequest>,
    ) -> Result<
        tonic::Response<tucana::aquila::FunctionDefinitionUpdateResponse>,
        tonic::Status,
    > {
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
