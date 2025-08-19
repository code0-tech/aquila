use crate::authorization::authorization::get_authorization_metadata;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Extensions;
use tonic::Request;
use tonic::transport::Channel;
use tucana::aquila::FlowTypeUpdateRequest as AquilaFlowTypeUpdateRequest;
use tucana::aquila::FlowTypeUpdateResponse as AquilaFlowTypeUpdateResponse;
use tucana::sagittarius::FlowTypeUpdateRequest as SagittariusFlowTypeUpdateRequest;
use tucana::sagittarius::flow_type_service_client::FlowTypeServiceClient;

pub struct SagittariusFlowTypeServiceClient {
    client: FlowTypeServiceClient<Channel>,
    token: String,
}

impl SagittariusFlowTypeServiceClient {
    pub async fn new_arc(sagittarius_url: String, token: String) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(sagittarius_url, token).await))
    }

    pub async fn new(sagittarius_url: String, token: String) -> Self {
        let client = match FlowTypeServiceClient::connect(sagittarius_url).await {
            Ok(client) => {
                log::info!("Successfully connected to Sagittarius FlowType Endpoint!");
                client
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (FlowType Endpoint): {:?}",
                err
            ),
        };

        Self { client, token }
    }

    pub async fn update_flow_types(
        &mut self,
        flow_type_update_request: AquilaFlowTypeUpdateRequest,
    ) -> AquilaFlowTypeUpdateResponse {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            SagittariusFlowTypeUpdateRequest {
                flow_types: flow_type_update_request.flow_types,
            },
        );

        let response = match self.client.update(request).await {
            Ok(response) => {
                log::info!(
                    "Successfully transferred FlowTypes. Did Sagittarius updated them? {:?}",
                    &response
                );
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update FlowTypes: {:?}", err);
                return AquilaFlowTypeUpdateResponse { success: false };
            }
        };

        AquilaFlowTypeUpdateResponse {
            success: response.success,
        }
    }
}
