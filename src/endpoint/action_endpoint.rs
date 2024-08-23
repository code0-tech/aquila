use futures::StreamExt;
use tonic::{Request, Response, Status, Streaming};
use crate::client::action_client::ActionClient;
use crate::external::external::action_transfer_service_server::ActionTransferService;
use crate::external::external::{InformationRequest, InformationResponse};

pub struct ActionEndpoint {
    client: ActionClient,
}

impl ActionTransferService for ActionEndpoint {
    async fn transfer(&mut self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        let mut identifier_option: Option<String> = None;
        let mut stream = request.into_inner();
        let mut first_request = false;

        while let Some(result) = stream.next().await {
            match result {
                Ok(info_request) => {
                    if !first_request {
                        first_request = true;
                        identifier_option = Some(info_request.identifier.clone());
                        self.client.logon(info_request)
                    }
                }
                Err(_) => {
                    return Err(Status::internal("Error receiving stream"));
                }
            }
        }

        if let Some(identifier) = identifier_option {
            self.client.logoff(identifier.clone());
            Ok(Response::new(InformationResponse { success: true }))
        } else {
            Err(Status::not_found("No valid request received"))
        }
    }
}