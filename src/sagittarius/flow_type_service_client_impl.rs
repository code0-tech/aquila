use crate::authorization::authorization::get_authorization_metadata;
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
    pub fn new(channel: Channel, token: String) -> Self {
        let client = FlowTypeServiceClient::new(channel);

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
                log::info!("Successfully transferred FlowTypes.",);
                response.into_inner()
            }
            Err(err) => {
                log::error!("Failed to update FlowTypes: {:?}", err);
                return AquilaFlowTypeUpdateResponse { success: false };
            }
        };

        match response.success {
            true => log::info!("Sagittarius successfully updated FlowTypes."),
            false => log::error!("Sagittarius didn't update any FlowTypes."),
        };

        AquilaFlowTypeUpdateResponse {
            success: response.success,
        }
    }
}
