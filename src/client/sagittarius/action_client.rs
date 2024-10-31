use async_trait::async_trait;
use log::{error, info};
use tonic::{Request};
use tonic::transport::Channel;
use tucana_internal::aquila::InformationRequest;
use tucana_internal::sagittarius::action_service_client::ActionServiceClient;
use tucana_internal::sagittarius::{ActionLogoffRequest, ActionLogonRequest};

pub struct SagittariusActionClientBase {
    client: ActionServiceClient<Channel>,
}

#[async_trait]
pub trait SagittariusActionClient {
    async fn new(sagittarius_url: String) -> SagittariusActionClientBase;
    async fn send_action_logon_request(&mut self, information: InformationRequest);
    async fn send_action_logoff_request(&mut self, identifier: String);
}

#[async_trait]
impl SagittariusActionClient for SagittariusActionClientBase {
    
    async fn new(sagittarius_url: String) -> SagittariusActionClientBase {
        let client = match ActionServiceClient::connect(sagittarius_url).await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {}", start_error);
            }
        };

        SagittariusActionClientBase { client }
    }

    async fn send_action_logon_request(&mut self, information: InformationRequest) {
        let request = Request::new(ActionLogonRequest {
            identifier: information.identifier,
            function_definition: information.function_definition,
            parameter_definition: information.parameter_definition,
        });

        match self.client.logon(request).await {
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
            },
            Ok(_) => {
                info!("Successfully reported action logon to sagittarius")
            }
        };
    }

    async fn send_action_logoff_request(&mut self, identifier: String) {
        let request = Request::new(ActionLogoffRequest {
            identifier
        });

        match self.client.logoff(request).await {
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
            },
            Ok(_) => {
                info!("Successfully reported action logoff to sagittarius")
            }
        };
    }
}