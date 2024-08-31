use std::sync::{Arc};
use futures::StreamExt;
use tokio::sync::Mutex;
use tonic::{async_trait, Request, Response, Status, Streaming};
use tucana_internal::actions::action_transfer_service_server::ActionTransferService;
use tucana_internal::actions::{InformationRequest, InformationResponse};
use crate::client::action_client::ActionClient;

pub struct ActionEndpoint {
    client_arc: Arc<Mutex<Box<ActionClient>>>,
}

impl ActionEndpoint {
    async fn receive_transfer(&self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        let mut identifier_option: Option<String> = None;
        let mut stream = request.into_inner();
        let mut first_request = false;

        while let Some(result) = stream.next().await {
            match result {
                Ok(info_request) => {
                    if !first_request {
                        first_request = true;
                        identifier_option = Some(info_request.identifier.clone());
                       
                        let mut client = self.client_arc.lock().await;
                        client.logon(info_request).await
                    }
                }
                Err(_) => {
                    return Err(Status::internal("Error receiving stream"));
                }
            }
        }

        if let Some(identifier) = identifier_option {
            let mut client = self.client_arc.lock().await;
            client.logoff(identifier.clone()).await;
            Ok(Response::new(InformationResponse { success: true }))
        } else {
            Err(Status::not_found("No valid request received"))
        }
    }
}

#[async_trait]
impl ActionTransferService for ActionEndpoint {
    
    async fn transfer(&self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        self.receive_transfer(request).await
    }
}