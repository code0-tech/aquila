use std::sync::Arc;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands};
use tokio::sync::Mutex;
use tonic::Request;
use crate::endpoint::configuration_endpoint::configuration_service_client::ConfigurationServiceClient;
use crate::endpoint::configuration_endpoint::{Configuration, Flow, GetConfigurationRequest};

pub struct FlowService {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
}

impl FlowService {
    pub fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> Self {
        Self { connection_arc }
    }

    pub async fn update_flow(&self, configuration_id: i64, flows: Vec<Flow>) {
        let mut connection = self.connection_arc.lock().await;

        for flow in flows {
            let id = format!("{}:{}", configuration_id, flow.flow_id);
            connection.set(id, serde_json::to_string(&flow).unwrap_or_else(|err| {
                panic!("Unable to update flow {id}: {err}")
            }));
        }
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