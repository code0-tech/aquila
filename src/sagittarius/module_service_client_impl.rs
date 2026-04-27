use crate::authorization::authorization::get_authorization_metadata;
use tonic::transport::Channel;
use tonic::{Extensions, Request};

pub struct SagittariusModuleServiceClient {
    client: tucana::sagittarius::module_service_client::ModuleServiceClient<Channel>,
    token: String,
}

impl SagittariusModuleServiceClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = tucana::sagittarius::module_service_client::ModuleServiceClient::new(channel);

        Self { client, token }
    }

    pub async fn update_modules(
        &mut self,
        modules_update_request: tucana::aquila::ModuleUpdateRequest,
    ) -> tucana::aquila::ModuleUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::ModuleUpdateRequest {
                modules: modules_update_request.modules,
            },
        );

        match self.client.update(request).await {
            Ok(response) => {
                let res = response.into_inner();
                match res.success {
                    true => log::info!("Sagittarius successfully updated Modules."),
                    false => log::error!(
                        "Sagittarius didn't update any Modules. Reason: {:?}",
                        res.error
                    ),
                };

                tucana::aquila::ModuleUpdateResponse {
                    success: res.success,
                }
            }
            Err(err) => {
                log::error!("Failed to update DataTypes: {:?}", err);
                tucana::aquila::ModuleUpdateResponse { success: false }
            }
        }
    }
}
