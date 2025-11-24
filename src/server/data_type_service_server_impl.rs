use crate::sagittarius::data_type_service_client_impl::SagittariusDataTypeServiceClient;
use std::sync::Arc;
use tokio::sync::Mutex;
use tucana::aquila::data_type_service_server::DataTypeService;

pub struct AquilaDataTypeServiceServer {
    client: Arc<Mutex<SagittariusDataTypeServiceClient>>,
}

impl AquilaDataTypeServiceServer {
    pub fn new(client: Arc<Mutex<SagittariusDataTypeServiceClient>>) -> Self {
        Self { client }
    }
}

#[tonic::async_trait]
impl DataTypeService for AquilaDataTypeServiceServer {
    async fn update(
        &self,
        request: tonic::Request<tucana::aquila::DataTypeUpdateRequest>,
    ) -> Result<tonic::Response<tucana::aquila::DataTypeUpdateResponse>, tonic::Status>
    {
        let data_type_update_request = request.into_inner();

        log::info!(
            "Received DataTypes: {:?}",
            data_type_update_request.data_types
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
