use crate::authorization::authorization::get_authorization_metadata;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Extensions;
use tonic::Request;
use tonic::transport::Channel;
use tucana::aquila::RuntimeFunctionDefinitionUpdateRequest as AquilaRuntimeFunctionUpdateRequest;
use tucana::aquila::RuntimeFunctionDefinitionUpdateResponse as AquilaRuntimeFunctionUpdateResponse;
use tucana::sagittarius::RuntimeFunctionDefinitionUpdateRequest as SagittariusRuntimeFunctionUpdateRequest;
use tucana::sagittarius::runtime_function_definition_service_client::RuntimeFunctionDefinitionServiceClient;

pub struct SagittariusRuntimeFunctionServiceClient {
    client: RuntimeFunctionDefinitionServiceClient<Channel>,
    token: String,
}

impl SagittariusRuntimeFunctionServiceClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = RuntimeFunctionDefinitionServiceClient::new(channel);
        Self { client, token }
    }

    pub fn new_arc(channel: Channel, token: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(channel, token)))
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
                log::info!("Successfully transferred RuntimeFunctions.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update RuntimeFunctions: {:?}", err);
                return AquilaRuntimeFunctionUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated RuntimeFunctions."),
            false => log::error!("Sagittarius didn't update any RuntimeFunctionRuntimeFunctions."),
        };

        AquilaRuntimeFunctionUpdateResponse {
            success: response.success,
        }
    }
}
