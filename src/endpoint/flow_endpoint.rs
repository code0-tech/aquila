use std::sync::Arc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisFuture};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use crate::endpoint::configuration_endpoint::{Flow, FlowDeleteRequest, FlowDeleteResponse, FlowUpdateRequest, FlowUpdateResponse};
use crate::endpoint::configuration_endpoint::flow_aquila_service_server::FlowAquilaService;

pub struct FlowEndpoint {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
}

impl FlowEndpoint {

    pub fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> Self {
        Self { connection_arc }
    }

    pub async fn update_flow(&self, flow: Flow) -> Result<Response<FlowUpdateResponse>, Status> {
        let mut connection = self.connection_arc.lock().await;

        let id = flow.flow_id.to_string();

        let serialized_flow = match serde_json::to_string(&flow) {
            Ok(result) => result,
            Err(error) => return Err(Status::internal(format!("Flow with id: {} wasn't serili because: {}", id, error)))
        };

        let operation = connection.set(id, serialized_flow);

        let has_changed = match operation.await
        {
            Ok(result) => result,
            Err(error) => return Err(Status::internal(format!("Flow with id: {} wasn't updated because: {}", id, error)))
        };

        return Ok(Response::new(FlowUpdateResponse {
            success: has_changed == "1"
        }));
    }

    pub async fn delete_flow(&self, flow_id: i64) -> Result<Response<FlowDeleteResponse>, Status> {
        let mut connection = self.connection_arc.lock().await;

        let id = flow_id.to_string();

        let operation: RedisFuture<String> = connection.del(id);

        let has_changed = match operation.await {
            Ok(result) => result,
            Err(error) => return Err(Status::internal(format!("Flow with id: {} wasn't deleted because: {}", id, error)))
        };

        return Ok(Response::new(FlowDeleteResponse {
            success: has_changed == "1"
        }));
    }
}

impl FlowAquilaService for FlowEndpoint {

    async fn update(&self, request: Request<FlowUpdateRequest>) -> Result<Response<FlowUpdateResponse>, Status> {
        let req = request.into_inner();
        self.update_flow(req.updated_flow.unwrap()).await
    }

    async fn delete(&self, request: Request<FlowDeleteRequest>) -> Result<Response<FlowDeleteResponse>, Status> {
        let req = request.into_inner();
        self.delete_flow(req.flow_id).await
    }
}