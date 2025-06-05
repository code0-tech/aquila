use crate::authorization::authorization::get_authorization_metadata;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Channel, Extensions, Request};
use tucana::sagittarius::{
    data_type_service_client::DataTypeServiceClient,
    DataTypeUpdateRequest as SagittariusDataTypeUpdateRequest,
};
use tucana::{
    aquila::DataTypeUpdateRequest as AquilaDataTypeUpdateRequest,
    aquila::DataTypeUpdateResponse as AquilaDataTypeUpdateResponse,
};
pub struct SagittariusDataTypeServiceClient {
    client: DataTypeServiceClient<Channel>,
    token: String,
}

impl SagittariusDataTypeServiceClient {
    pub async fn new_arc(sagittarius_url: String, token: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(sagittarius_url, token).await))
    }

    pub async fn new(sagittarius_url: String, token: String) -> Self {
        let client = match DataTypeServiceClient::connect(sagittarius_url).await {
            Ok(client) => {
                log::info!("Successfully connected to Sagittarius DataType Endpoint!");
                client
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (DataType Endpoint): {:?}",
                err
            ),
        };

        Self { client, token }
    }

    pub async fn update_data_types(
        &mut self,
        data_type_update_request: AquilaDataTypeUpdateRequest,
    ) -> AquilaDataTypeUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            SagittariusDataTypeUpdateRequest {
                data_types: data_type_update_request.data_types,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!(
                    "Successfully transferred data types. Did Sagittarius updated them? {:?}",
                    &response
                );
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update DataTypes: {:?}", err);
                return AquilaDataTypeUpdateResponse { success: false };
            }
        };

        AquilaDataTypeUpdateResponse {
            success: response.success,
        }
    }
}
