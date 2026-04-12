use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::flow_type_service_server::FlowTypeService;

use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration, sagittarius::flow_type_service_client_impl::SagittariusFlowTypeServiceClient
};

pub struct AquilaFlowTypeServiceServer {
    client: Arc<Mutex<SagittariusFlowTypeServiceClient>>,
    service_configuration: ServiceConfiguration,
}

impl AquilaFlowTypeServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusFlowTypeServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl FlowTypeService for AquilaFlowTypeServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::FlowTypeUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::FlowTypeUpdateResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => t,
            Err(status) => return Err(status),
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            return Err(Status::unauthenticated("token is not valid"));
        }

        let flow_type_update_request = request.into_inner();

        log::debug!(
            "Received FlowTypes: {:?}",
            flow_type_update_request
                .flow_types
                .iter()
                .map(|f| f.identifier.clone())
                .collect::<Vec<_>>()
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
