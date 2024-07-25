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

    pub async fn delete_flow(&self, configuration_id: i64, flows: Vec<Flow>) {
        let mut connection = self.connection_arc.lock().await;

        for flow in flows {
            let id = format!("{}:{}", configuration_id, flow.flow_id);
            connection.del(id);
        }
    }

    pub async fn get_flows(&self, client: &mut ConfigurationServiceClient<Configuration>) {
        let mut connection = self.connection_arc.lock().await;

        let request = Request::new(GetConfigurationRequest {
            configuration_id: 91
        });

        let response = ConfigurationServiceClient::get(client, request).await;
        //TODO: store recieved flows into Redis
    }
}