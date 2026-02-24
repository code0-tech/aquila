use crate::sagittarius::runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient;
use std::sync::Arc;
use tokio::sync::Mutex;
use tucana::aquila::runtime_status_service_server::RuntimeStatusService;

pub struct AquilaRuntimeStatusServiceServer {
    client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>,
}

impl AquilaRuntimeStatusServiceServer {
    pub fn new(client: Arc<Mutex<SagittariusRuntimeStatusServiceClient>>) -> Self {
        Self { client }
    }
}

#[tonic::async_trait]
impl RuntimeStatusService for AquilaRuntimeStatusServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::RuntimeStatusUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::RuntimeStatusUpdateResponse>, tonic::Status> {
        let runtime_status_update_request = request.into_inner();

        log::debug!("Received Runtime Status Update");

        let mut client = self.client.lock().await;
        let response = client
            .update_runtime_status(runtime_status_update_request)
            .await;

        Ok(tonic::Response::new(
            tucana::aquila::RuntimeStatusUpdateResponse {
                success: response.success,
            },
        ))
    }
}
