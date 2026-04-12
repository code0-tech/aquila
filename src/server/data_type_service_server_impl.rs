use crate::{
    authorization::authorization::extract_token, configuration::service::ServiceConfiguration,
    sagittarius::data_type_service_client_impl::SagittariusDataTypeServiceClient,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Status;
use tucana::aquila::data_type_service_server::DataTypeService;

pub struct AquilaDataTypeServiceServer {
    service_configuration: ServiceConfiguration,
    client: Arc<Mutex<SagittariusDataTypeServiceClient>>,
}

impl AquilaDataTypeServiceServer {
    pub fn new(
        client: Arc<Mutex<SagittariusDataTypeServiceClient>>,
        service_configuration: ServiceConfiguration,
    ) -> Self {
        Self {
            client,
            service_configuration,
        }
    }
}

#[tonic::async_trait]
impl DataTypeService for AquilaDataTypeServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::DataTypeUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::DataTypeUpdateResponse>, tonic::Status> {
        let token = match extract_token(&request) {
            Ok(t) => {
                log::debug!("Found token: {}", t);
                t
            },
            Err(status) => return Err(status),
        };

        if !self.service_configuration.has_service(&token.to_string()) {
            return Err(Status::unauthenticated("token is not valid"));
        }

        let data_type_update_request = request.into_inner();

        log::debug!(
            "Received DataTypes: {:?}",
            data_type_update_request
                .data_types
                .iter()
                .map(|d| d.identifier.clone())
                .collect::<Vec<_>>()
        );

        let mut client = self.client.lock().await;
        let response = client.update_data_types(data_type_update_request).await;

        Ok(tonic::Response::new(
            tucana::aquila::DataTypeUpdateResponse {
                success: response.success,
            },
        ))
    }
}
