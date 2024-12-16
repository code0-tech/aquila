use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status, Streaming};
use tucana::aquila::action_transfer_service_server::ActionTransferService;
use tucana::aquila::{ActionExecuteRequest, InformationRequest, InformationResponse};
use crate::service::action_service::{ActionService, ActionServiceBase};

pub struct ActionTransferServerBase {
    action_service: Arc<Mutex<ActionServiceBase>>,
}

pub type ExecuteStream = ();

/// gRPC Service Implementation
#[async_trait]
impl ActionTransferService for ActionTransferServerBase {
    /// Transfers `Flows` redivided from the `Action` to `Sagittarius`
    async fn transfer(&self, request: Request<Streaming<InformationRequest>>) -> Result<Response<InformationResponse>, Status> {
        let mut service = self.action_service.lock().await;
        service.transfer_action_flows(request).await
    }

    type ExecuteStream = ();

    async fn execute(&self, request: Request<Streaming<ActionExecuteRequest>>) -> Result<Response<Self::ExecuteStream>, Status> {
        todo!()
    }
}