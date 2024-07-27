use std::ops::Add;
use std::sync::Arc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisFuture};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use crate::endpoint::configuration_endpoint::{Flow, FlowDeleteResponse, FlowGetRequest, FlowUpdateResponse};
use crate::endpoint::configuration_endpoint::flow_service_client::FlowServiceClient;

pub struct BaseFlowService {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    client: FlowServiceClient<Flow>,
}

impl BaseFlowService {

    pub fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, client: FlowServiceClient<Flow>) -> Self {
        Self { connection_arc, client }
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

    pub async fn send_get_flow_request(&self) -> Request<FlowGetRequest> {
        let mut connection = self.connection_arc.lock().await;

        let string_keys: Vec<String> = connection.keys("*").await.expect("Failed to fetch keys");
        let int_keys: Vec<i64> = Vec::new();

        for key in string_keys {
            if let Ok(int_key) = key.parse::<i64>() {
                int_key.add(int_key);
            }
        }

        return Request::new(FlowGetRequest {
            flow_ids: int_keys
        });
    }

    pub async fn handle_get_flow_request(&self, update_flows: Vec<Flow>, deleted_flow_ids: Vec<i64>) {
        let mut connection = self.connection_arc.lock().await;

        connection.del(deleted_flow_ids);

        for flow in update_flows {
            let serialized_flow = serde_json::to_string(&flow);

            match serialized_flow {
                Ok(value) => {
                    connection.set(flow.flow_id.to_string(), value);
                }

                Err(_) => continue
            }
        }
    }
}