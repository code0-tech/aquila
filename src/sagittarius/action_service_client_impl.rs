use tonic::{transport::Channel, Extensions, Request, Response};
use tucana::{
    sagittarius::{
        action_service_client::ActionServiceClient, ActionLogoffRequest, ActionLogoffResponse,
        ActionLogonRequest, ActionLogonResponse,
    },
    shared::RuntimeFunctionDefinition,
};

use crate::authorization::authorization::get_authorization_metadata;

/// Struct representing a service for sending flows received from an `Action` to `Sagittarius`.
/// Part that informs `Sagittarius`
pub struct SagittariusActionClient {
    client: ActionServiceClient<Channel>,
    token: String,
}

/// Implementation of the service for sending flows received from an `Action` to `Sagittarius`.
/// Part that informs `Sagittarius`
impl SagittariusActionClient {
    /// Creates a connection to `Sagittarius`
    ///
    /// Behavior:
    /// Will panic when a connection can`t be established
    pub async fn new(sagittarius_url: String, token: String) -> SagittariusActionClient {
        let client = match ActionServiceClient::connect(sagittarius_url).await {
            Ok(res) => {
                log::info!("Successfully connected to Sagittarius Action Endpoint!");
                res
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (Action Endpoint): {:?}",
                err
            ),
        };

        SagittariusActionClient { client, token }
    }

    /// Sends `Sagittarius` the information that a `Action` went online.
    pub async fn send_action_logon_request(
        &mut self,
        identifier: String,
        function_definition: Vec<RuntimeFunctionDefinition>,
    ) -> Result<Response<ActionLogonResponse>, tonic::Status> {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ActionLogonRequest {
                identifier,
                function_definition,
            },
        );

        match self.client.logon(request).await {
            Err(status) => {
                log::error!("Received a {:?}, can't logon the Action!", status);
                Err(status)
            }
            Ok(response) => {
                print!("Successfully reported an Action logon to Sagittarius");
                Ok(response)
            }
        }
    }

    /// Sends `Sagittarius` the information that a `Action` went offline.
    pub async fn send_action_logoff_request(
        &mut self,
        identifier: String,
    ) -> Result<Response<ActionLogoffResponse>, tonic::Status> {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            ActionLogoffRequest { identifier },
        );

        match self.client.logoff(request).await {
            Err(status) => {
                log::error!("Received a {status}, can't logoff the Action!");
                Err(status)
            }
            Ok(response) => {
                log::info!("Successfully reported Action logoff to Sagittarius");
                Ok(response)
            }
        }
    }
}
