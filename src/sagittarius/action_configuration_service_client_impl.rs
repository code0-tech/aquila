use crate::authorization::authorization::get_authorization_metadata;
use tonic::transport::Channel;
use tonic::{Extensions, Request};

pub struct SagittariusActionConfigurationServiceClient {
    client:
        tucana::sagittarius::action_configuration_service_client::ActionConfigurationServiceClient<
            Channel,
        >,
    token: String,
}

impl SagittariusActionConfigurationServiceClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = tucana::sagittarius::action_configuration_service_client::ActionConfigurationServiceClient::new(channel);

        Self { client, token }
    }

    pub async fn update_action_configuration(
        &mut self,
        action_configuration_update_request: tucana::aquila::ActionConfigurationUpdateRequest,
    ) -> tucana::aquila::ActionConfigurationUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::ActionConfigurationUpdateRequest {
                action_identifier: action_configuration_update_request.action_identifier,
                action_configurations: action_configuration_update_request.action_configurations,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Successfully transferred action configuration update.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update action configurations: {:?}", err);
                return tucana::aquila::ActionConfigurationUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated ActionConfiguration."),
            false => log::error!("Sagittarius didn't update any ActionConfiguration."),
        };

        tucana::aquila::ActionConfigurationUpdateResponse {
            success: response.success,
        }
    }
}
