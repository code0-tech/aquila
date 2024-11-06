use std::sync::Arc;
use futures::StreamExt;
use log::error;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status, Streaming};
use tucana_internal::aquila::{InformationRequest, InformationResponse};
use crate::client::sagittarius::action_client::{SagittariusActionClient, SagittariusActionClientBase};

pub struct ActionServiceBase {
    sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>
}

pub trait ActionService {
    async fn new(sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>) -> ActionServiceBase;
    async fn transfer_action_flows(&mut self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status>;
}

impl ActionService for ActionServiceBase {
    
    async fn new(sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>) -> ActionServiceBase {
        ActionServiceBase { sagittarius_client }
    }

    async fn transfer_action_flows(&mut self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        let mut identifier_option: Option<String> = None;
        let mut stream = request.into_inner();
        let mut first_request = false;

        while let Some(result) = stream.next().await {
            match result {
                Ok(info_request) => {
                    if !first_request {
                        first_request = true;
                        identifier_option = Some(info_request.identifier.clone());

                        let mut client = self.sagittarius_client.lock().await;
                        let result = client.send_action_logon_request(info_request).await;
                        if result.is_err() {
                            return Err(result.err().unwrap().into());
                        }
                    }
                }
                Err(status) => {
                    error!("Received a {status}, can't retrieve flows from Sagittarius");
                    return Err(Status::internal("Error receiving stream"));
                }
            }
        }

        if let Some(identifier) = identifier_option {
            let mut client = self.sagittarius_client.lock().await;
            client.send_action_logoff_request(identifier.clone()).await;
            Ok(Response::new(InformationResponse { success: true }))
        } else {
            Err(Status::not_found("No valid request received"))
        }
    }
}

mod tests {
    //TODO: Write tests
}