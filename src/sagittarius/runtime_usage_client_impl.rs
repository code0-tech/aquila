use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius::runtime_usage_service_client::RuntimeUsageServiceClient as SagittariusRuntimeUsageServiceClient;

use crate::authorization::authorization::get_authorization_metadata;

pub struct SagittariusRuntimeUsageClient {
    client: SagittariusRuntimeUsageServiceClient<Channel>,
    token: String,
}

impl SagittariusRuntimeUsageClient {
    pub fn new(channel: Channel, token: String) -> Self {
        let client = SagittariusRuntimeUsageServiceClient::new(channel);
        Self { client, token }
    }

    pub fn new_arc(channel: Channel, token: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(channel, token)))
    }

    pub async fn update_runtime_usage(
        &mut self,
        runtime_usage_request: tucana::aquila::RuntimeUsageRequest,
    ) -> tucana::aquila::RuntimeUsageResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            tucana::sagittarius::RuntimeUsageRequest {
                runtime_usage: runtime_usage_request.runtime_usage,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!("Successfully transferred Runtime Usages.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update Runtime Usage: {:?}", err);
                return tucana::aquila::RuntimeUsageResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated RuntimeUsage."),
            false => log::error!("Sagittarius didn't update RuntimeUsage."),
        };

        tucana::aquila::RuntimeUsageResponse {
            success: response.success,
        }
    }
}
