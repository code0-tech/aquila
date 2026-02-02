use crate::authorization::authorization::get_authorization_metadata;
use tonic::transport::Channel;
use tonic::{Extensions, Request};
use tucana::sagittarius::{
    DataTypeUpdateRequest as SagittariusDataTypeUpdateRequest,
    data_type_service_client::DataTypeServiceClient,
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
    pub fn new(channel: Channel, token: String) -> Self {
        let client = DataTypeServiceClient::new(channel);

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
                log::info!("Successfully transferred data types.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update DataTypes: {:?}", err);
                return AquilaDataTypeUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated DataTypes."),
            false => log::error!("Sagittarius didn't update any DataTypes."),
        };

        AquilaDataTypeUpdateResponse {
            success: response.success,
        }
    }
}
