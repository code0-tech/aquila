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
            Ok(client) => client,
            Err(err) => panic!("Failed to connect to Sagittarius: {}", err),
        };

        Self { client, token }
    }

    pub async fn update_data_types(
        &mut self,
        data_type_update_request: AquilaDataTypeUpdateRequest,
    ) -> AquilaDataTypeUpdateResponse {
        println!(
            "Recieved DataTypes: {:?}",
            &data_type_update_request.data_types
        );

        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            SagittariusDataTypeUpdateRequest {
                data_types: data_type_update_request.data_types,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => response,
            Err(_) => return AquilaDataTypeUpdateResponse { success: false },
        };

        AquilaDataTypeUpdateResponse {
            success: response.into_inner().success,
        }
    }
}

#[cfg(test)]
mod tests {
    use tonic::transport::Server;

    struct MockSagittarius {
        will_succseed: bool,
    }

    impl MockSagittarius {
        pub fn new(will_succseed: bool) -> Self {
            Self { will_succseed }
        }
    }

    #[tonic::async_trait]
    impl tucana::sagittarius::data_type_service_server::DataTypeService for MockSagittarius {
        async fn update(
            &self,
            _: tonic::Request<tucana::sagittarius::DataTypeUpdateRequest>,
        ) -> std::result::Result<
            tonic::Response<tucana::sagittarius::DataTypeUpdateResponse>,
            tonic::Status,
        > {
            Ok(tonic::Response::new(
                tucana::sagittarius::DataTypeUpdateResponse {
                    success: self.will_succseed,
                },
            ))
        }
    }

    #[tokio::test]
    async fn test_update_data_types() {
        let address = "[::1]:8080".parse().unwrap();
        let mock_sagittarius = MockSagittarius::new(true);

        let sagittarius_server = Server::builder()
            .add_service(
                tucana::sagittarius::data_type_service_server::DataTypeServiceServer::new(
                    mock_sagittarius,
                ),
            )
            .serve(address)
            .await;

        assert!(sagittarius_server.is_ok());

        todo!()
    }
}
