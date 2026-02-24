use std::sync::Arc;

use tokio::sync::Mutex;
use tucana::aquila::runtime_usage_service_server::RuntimeUsageService;

use crate::sagittarius::runtime_usage_client_impl::SagittariusRuntimeUsageClient;

pub struct AquilaRuntimeUsageServiceServer {
    client: Arc<Mutex<SagittariusRuntimeUsageClient>>,
}

impl AquilaRuntimeUsageServiceServer {
    pub fn new(client: Arc<Mutex<SagittariusRuntimeUsageClient>>) -> Self {
        Self { client }
    }
}

#[tonic::async_trait]
impl RuntimeUsageService for AquilaRuntimeUsageServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeUsageRequest>,
    ) -> Result<tonic::Response<tucana::aquila::RuntimeUsageResponse>, tonic::Status> {
        let runtime_usage_request = request.into_inner();

        log::debug!("Received RuntimeUsageRequest",);

        let mut client = self.client.lock().await;
        let response = client
            .update_runtime_usage(runtime_usage_request)
            .await;

        Ok(tonic::Response::new(tucana::aquila::RuntimeUsageResponse {
            success: response.success,
        }))
    }
}
