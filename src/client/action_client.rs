use log::error;
use tonic::{Request};
use tonic::transport::Channel;
use crate::external::external::{ActionLogoffRequest, ActionLogonRequest, InformationRequest};
use crate::external::external::action_service_client::ActionServiceClient;
use crate::external::external::action_service_server::ActionService;

pub struct ActionClient {
    client: ActionServiceClient<Channel>,
}

impl ActionClient {

    pub async fn new() -> Self {
        let client = ActionServiceClient::connect("https://[::1]:50051")
            .await
            .expect("Cannot connect to service");

        Self { client }
    }

    pub async fn logon(&mut self, information: InformationRequest) {
        let request = Request::new(ActionLogonRequest {
            identifier: information.identifier,
            function_definition: information.function_definition,
            parameter_definition: information.parameter_definition,
        });

        match self.client.logon(request) {
            Err(err) => {
                error!("Failed to send logon request");
            }
        };
    }

    pub fn logoff(&mut self, identifier: String) {
        let request = Request::new(ActionLogoffRequest {
            identifier
        });

        match self.client.logoff(request) {
            Err(err) => {
                error!("Failed to send logoff request");
            }
        };
    }
}