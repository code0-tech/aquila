use crate::authorization::authorization::get_authorization_metadata;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tonic::Extensions;
use tonic::Request;
use tucana::aquila::FlowTypeUpdateRequest as AquilaFlowTypeUpdateRequest;
use tucana::aquila::FlowTypeUpdateResponse as AquilaFlowTypeUpdateResponse;
use tucana::sagittarius::flow_type_service_client::FlowTypeServiceClient;
use tucana::sagittarius::FlowTypeUpdateRequest as SagittariusFlowTypeUpdateRequest;

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
            Ok(client) => client,
            Err(err) => panic!("Failed to connect to Sagittarius: {}", err),
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
            Ok(response) => response.into_inner(),
            Err(_) => return AquilaFlowTypeUpdateResponse { success: false },
        };

        AquilaFlowTypeUpdateResponse {
            success: response.success,
        }
    }
}
