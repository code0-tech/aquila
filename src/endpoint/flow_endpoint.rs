use std::sync::Arc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisFuture};
use tokio::sync::Mutex;
use tonic::{async_trait, Request, Response, Status};
use tucana_internal::internal::{Flow, FlowDeleteRequest, FlowDeleteResponse, FlowUpdateRequest, FlowUpdateResponse};
use tucana_internal::internal::flow_aquila_service_server::FlowAquilaService;

pub struct FlowEndpoint {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
}

impl FlowEndpoint {
    pub fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> Self {
        Self { connection_arc }
    }

    pub async fn update_flow(&self, flow: Flow) -> Result<Response<FlowUpdateResponse>, Status> {
        let mut connection = self.connection_arc.lock().await;

        let id = &flow.flow_id.to_string();

        let serialized_flow = match serde_json::to_string(&flow) {
            Ok(result) => result,
            Err(error) => return Err(Status::internal(format!("Flow with id: {} wasn't serialized because: {}", id, error)))
        };

        let operation = connection.set(id.to_string(), serialized_flow);

        match operation.await {
            Ok(success) => Ok(Response::new(FlowUpdateResponse { success })),
            Err(err) => Err(Status::internal(format!("Flow with id: {} wasn't updated because: {}", id, err)))
        }
    }

    pub async fn delete_flow(&self, flow_id: i64) -> Result<Response<FlowDeleteResponse>, Status> {
        let mut connection = self.connection_arc.lock().await;

        let id = &flow_id.to_string();

        let operation: RedisFuture<String> = connection.del(id);

        match operation.await {
            Ok(success_str) => Ok(Response::new(FlowDeleteResponse { success: success_str.eq("1") })),
            Err(err) => Err(Status::internal(format!("Flow with id: {} wasn't deleted because: {}", id, err)))
        }
    }
}

#[async_trait]
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

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_update_flow() {
        todo!()
    }
}