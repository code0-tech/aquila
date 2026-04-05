use crate::authorization::authorization::get_authorization_metadata;
use tonic::Extensions;
use tonic::Request;
use tonic::transport::Channel;
use tucana::sagittarius::function_definition_service_client::FunctionDefinitionServiceClient;

pub struct SagittariusFunctionDefinitionServiceClient {
    client: FunctionDefinitionServiceClient<Channel>,
    token: String,
}

impl SagittariusFunctionDefinitionServiceClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = FunctionDefinitionServiceClient::new(channel);
        Self { client, token }
    }

    pub async fn update_function_definitions(
        &mut self,
        function_update_request: tucana::aquila::FunctionDefinitionUpdateRequest,
    ) -> tucana::aquila::FunctionDefinitionUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::FunctionDefinitionUpdateRequest {
                functions: function_update_request.functions,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Successfully transferred Functions.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update Functions: {:?}", err);
                return tucana::aquila::FunctionDefinitionUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated Functions."),
            false => log::error!("Sagittarius didn't update any Functions."),
        };

        tucana::aquila::FunctionDefinitionUpdateResponse {
            success: response.success,
        }
    }
}
