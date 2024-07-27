use tonic::{Request, Response, Status};
use crate::endpoint::configuration_endpoint::{Flow, FlowDeleteRequest, FlowDeleteResponse, FlowGetRequest, FlowGetResponse, FlowUpdateRequest, FlowUpdateResponse};
use crate::endpoint::configuration_endpoint::flow_service_server::{FlowService, FlowServiceServer};
use crate::service::flow_service::{BaseFlowService};

impl FlowService for BaseFlowService {

    async fn update(&self, request: Request<FlowUpdateRequest>) -> Result<Response<FlowUpdateResponse>, Status> {
        let req = request.into_inner();
        self.update_flow(req.updated_flow.unwrap()).await
    }

    async fn delete(&self, request: Request<FlowDeleteRequest>) -> Result<Response<FlowDeleteResponse>, Status> {
        let req = request.into_inner();
        self.delete_flow(req.flow_id).await
    }

    async fn get(&self, request: Request<FlowGetRequest>) -> Result<Response<FlowGetResponse>, Status> {
        todo!()
    }

}