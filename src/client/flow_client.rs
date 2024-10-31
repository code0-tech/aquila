use std::sync::Arc;
use futures::StreamExt;
use log::error;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tonic::Request;
use tonic::transport::Channel;
use tucana_internal::sagittarius::flow_service_client::FlowServiceClient;
use tucana_internal::sagittarius::{Flow, FlowCommandType, FlowGetRequest, FlowLogonRequest};

const INSERT: i32 = FlowCommandType::Insert as i32;
const DELETE: i32 = FlowCommandType::Delete as i32;

#[derive(Clone)]
pub struct FlowClient {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    client: FlowServiceClient<Channel>,
}

impl FlowClient {
    pub async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>, client: FlowServiceClient<Channel>) -> Self {
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

    pub async fn insert_flow(&mut self, flow: Flow) {
        let mut connection = self.connection_arc.lock().await;

        let serialized_flow = serde_json::to_string(&flow);

        match serialized_flow {
            Ok(parsed_flow) => {
                connection.set::<String, String, i64>(flow.flow_id.to_string(), parsed_flow);
            }
            Err(_) => {}
        }
    }

    pub async fn delete_flows(&self, deleted_flow_ids: Vec<i64>) {
        let mut connection = self.connection_arc.lock().await;
        connection.del::<Vec<i64>, i64>(deleted_flow_ids);
    }

    pub async fn delete_flow(&mut self, deleted_flow_id: i64) {
        let mut connection = self.connection_arc.lock().await;
        connection.del::<i64, i64>(deleted_flow_id);
    }

    pub async fn send_get_flow_request(&mut self) {
        let string_keys: Vec<String> = {
            let mut connection = self.connection_arc.lock().await;
            match connection.keys("*").await {
                Ok(res) => res,
                Err(error) => {
                    print!("Can't retrieve keys from redis. Reason: {error}");
                    return;
                }
            }
        };

        let int_keys: Vec<i64> = string_keys
            .into_iter()
            .filter_map(|key| key.parse::<i64>().ok())
            .collect();

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

        let update_flows = response.updated_flows;
        let deleted_flow_ids = response.deleted_flow_ids;
        self.insert_flows(update_flows).await;
        self.delete_flows(deleted_flow_ids).await
    }

    pub async fn logon(&mut self) {
        let response = self.client.update(Request::new(FlowLogonRequest {})).await.expect("TODO: panic message");
        let mut stream = response.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(info_request) => {
                    match info_request.r#type {
                        INSERT => {
                            
                            let flow = info_request.updated_flow;
                            if flow.is_none() {
                                error!("Recieved flow update request without flows");
                                continue;
                            }
                            
                            self.insert_flow(flow.unwrap()).await
                        },
                        DELETE => {

                            let id = info_request.deleted_flow_id;
                            if id.is_none() {
                                error!("Revieved flow delete request without id");
                                continue;
                            }
                            
                            self.delete_flow(id.unwrap()).await
                        }
                        _ => todo!(),
                    }
                }
                Err(_) => {}
            }
        }
    }
}