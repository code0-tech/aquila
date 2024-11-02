use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tonic::{Response, Status, Streaming};
use tucana_internal::aquila::action_transfer_service_server::ActionTransferService;
use tucana_internal::aquila::{InformationRequest, InformationResponse};
use crate::service::action_service::{ActionService, ActionServiceBase};

pub struct ActionTransferServerBase {
    action_service: Arc<Mutex<ActionServiceBase>>
}

#[async_trait]
impl ActionTransferService for ActionTransferServerBase {

    async fn transfer(&self, request: tonic::Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
       let mut service = self.action_service.lock().await;
       service.transfer_action_flows(request).await
    }
}

mod tests {
    //TODO: Write tests
}