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
        let module_count = modules_update_request.modules.len();
        log::debug!(
            "Forwarding module update to Sagittarius module_count={}",
            module_count
        );

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
                    true => log::info!(
                        "Sagittarius successfully updated modules module_count={}",
                        module_count
                    ),
                    false => log::warn!(
                        "Sagittarius rejected module update module_count={} reason={:?}",
                        module_count,
                        res.error
                    ),
                };

                tucana::aquila::ModuleUpdateResponse {
                    success: res.success,
                }
            }
            Err(err) => {
                log::error!(
                    "Failed to update Modules via Sagittarius RPC transport: {:?}",
                    err
                );
                tucana::aquila::ModuleUpdateResponse { success: false }
            }
        }
    }
}
