use std::sync::Arc;
use futures::StreamExt;
use log::error;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status, Streaming};
use tucana::aquila::{InformationRequest, InformationResponse};
use crate::client::sagittarius::action_client::{SagittariusActionClient, SagittariusActionClientBase};

/// Struct representing a service for sending flows received from an `Action` to `Sagittarius`.
/// Part that accepts `Action` requests.
pub struct ActionServiceBase {
    sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>,
}

/// Trait representing a service for sending flows received from an `Action` to `Sagittarius`.
/// Part that accepts `Action` requests.
pub trait ActionService {
    async fn new(sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>) -> ActionServiceBase;
    async fn transfer_action_flows(&mut self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status>;
}

/// Implementation of the service for sending flows received from an `Action` to `Sagittarius`.
/// Part that accepts `Action` requests.
impl ActionService for ActionServiceBase {

    async fn new(sagittarius_client: Arc<Mutex<Box<SagittariusActionClientBase>>>) -> ActionServiceBase {
        ActionServiceBase { sagittarius_client }
    }

    /// gRPC Function Implementation
    /// Transfers `Flows` redivided from the `Action` to `Sagittarius`
    async fn transfer_action_flows(&mut self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        let mut first_request = false;
        let mut identifier_option: Option<String> = None;
        let mut stream = request.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(info_request) => {

                    /// Information for `Sagittarius` that a new `Action` is online.
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

        /// The stream ended at this point. Now `Sagittarius` will be informed, that the `Action` is offline.
        if let Some(identifier) = identifier_option {
            let mut client = self.sagittarius_client.lock().await;
            client.send_action_logoff_request(identifier.clone()).await?;
            Ok(Response::new(InformationResponse { success: true }))
        } else {
            Err(Status::not_found("No valid request received"))
        }
    }
}
