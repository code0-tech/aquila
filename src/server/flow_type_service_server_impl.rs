use std::sync::Arc;
use tokio::sync::Mutex;
use tucana::aquila::flow_type_service_server::FlowTypeService;

use crate::sagittarius::flow_type_service_client_impl::SagittariusFlowTypeServiceClient;

pub struct AquilaFlowTypeServiceServer {
    client: Arc<Mutex<SagittariusFlowTypeServiceClient>>,
}

impl AquilaFlowTypeServiceServer {
    pub fn new(client: Arc<Mutex<SagittariusFlowTypeServiceClient>>) -> Self {
        Self { client }
    }
}

#[tonic::async_trait]
impl FlowTypeService for AquilaFlowTypeServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::FlowTypeUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::FlowTypeUpdateResponse>, tonic::Status>
    {
        let flow_type_update_request = request.into_inner();

        log::info!(
            "Received FlowTypes: {:?}",
            flow_type_update_request.flow_types
        );

        let mut client = self.client.lock().await;
        let response = client.update_flow_types(flow_type_update_request).await;

        Ok(tonic::Response::new(
            tucana::aquila::FlowTypeUpdateResponse {
                success: response.success,
            },
        ))
    }
}
