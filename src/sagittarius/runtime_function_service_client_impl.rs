use crate::authorization::authorization::get_authorization_metadata;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tonic::Extensions;
use tonic::Request;
use tucana::aquila::RuntimeFunctionDefinitionUpdateRequest as AquilaRuntimeFunctionUpdateRequest;
use tucana::aquila::RuntimeFunctionDefinitionUpdateResponse as AquilaRuntimeFunctionUpdateResponse;
use tucana::sagittarius::runtime_function_definition_service_client::RuntimeFunctionDefinitionServiceClient;
use tucana::sagittarius::RuntimeFunctionDefinitionUpdateRequest as SagittariusRuntimeFunctionUpdateRequest;

pub struct SagittariusRuntimeFunctionServiceClient {
    client: RuntimeFunctionDefinitionServiceClient<Channel>,
    token: String,
}

impl SagittariusRuntimeFunctionServiceClient {
    pub async fn new(sagittarius_url: String, token: String) -> Self {
        let client = match RuntimeFunctionDefinitionServiceClient::connect(sagittarius_url).await {
            Ok(client) => {
                log::info!("Successfully connected to Sagittarius RuntimeFunction Endpoint!");
                client
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (RuntimeFunction Endpoint): {:?}",
                err
            ),
        };

        Self { client, token }
    }

    pub async fn new_arc(sagittarius_url: String, token: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(sagittarius_url, token).await))
    }

    pub async fn update_runtime_function_definitions(
        &mut self,
        runtime_function_update_request: AquilaRuntimeFunctionUpdateRequest,
    ) -> AquilaRuntimeFunctionUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            SagittariusRuntimeFunctionUpdateRequest {
                runtime_functions: runtime_function_update_request.runtime_functions,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!(
                    "Successfully transferred RuntimeFunctions. Did Sagittarius updated them? {:?}",
                    &response
                );
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update RuntimeFunctions: {:?}", err);
                return AquilaRuntimeFunctionUpdateResponse { success: false };
            }
        };

        AquilaRuntimeFunctionUpdateResponse {
            success: response.success,
        }
    }
}
