use std::ops::Add;
use std::sync::Arc;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tonic::Request;
use tonic::transport::Channel;
use tucana_internal::internal::flow_sagittarius_service_client::FlowSagittariusServiceClient;
use tucana_internal::internal::{Flow, FlowGetRequest};

#[derive(Clone)]
pub struct FlowClient {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    client: FlowSagittariusServiceClient<Channel>,
}

impl FlowClient {
    pub async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, client: FlowSagittariusServiceClient<Channel>) -> Self {
        Self { connection_arc, client }
    }

    pub async fn insert_flows(&mut self, flows: Vec<Flow>) {
        let mut connection = self.connection_arc.lock().await;

        for flow in flows {
            let serialized_flow = serde_json::to_string(&flow);

            match serialized_flow {
                Ok(parsed_flow) => {
                    connection.set::<String, String, i64>(flow.flow_id.to_string(), parsed_flow);
                }
                Err(_) => continue
            }
        }
    }

    pub async fn send_get_flow_request(&mut self) {
        let mut connection = self.connection_arc.lock().await;

        let string_keys: Vec<String> = match connection.keys("*").await {
            Ok(res) => res,
            Err(error) => {
                print!("Can't retrieve keys from redis. Reason: {error}");
                return;
            }
        };

        let int_keys: Vec<i64> = Vec::new();

        for key in string_keys {
            if let Ok(int_key) = key.parse::<i64>() {
                int_key.add(int_key);
            }
        }

        let request = Request::new(FlowGetRequest {
            flow_ids: int_keys
        });

        let response = match self.client.get(request).await {
            Ok(res) => res.into_inner(),
            Err(status) => {
                print!("Received a {status}, can't retrieve flows from Sagittarius");
                return;
            }
        };

        self.handle_get_flow_request(
            response.updated_flows,
            response.deleted_flow_ids,
        ).await;
    }

    pub async fn handle_get_flow_request(&self, update_flows: Vec<Flow>, deleted_flow_ids: Vec<i64>) {
        let mut connection = self.connection_arc.lock().await;

        //todo look over RV generic on redis actions
        connection.del::<Vec<i64>, i64>(deleted_flow_ids);

        for flow in update_flows {
            let serialized_flow = serde_json::to_string(&flow);

            match serialized_flow {
                Ok(value) => {
                    connection.set::<String, String, i64>(flow.flow_id.to_string(), value);
                }

                Err(_) => continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_send_get_flow_request() {
        todo!()
    }

    #[tokio::test]
    async fn test_handle_get_flow_request() {
        todo!()
    }
}