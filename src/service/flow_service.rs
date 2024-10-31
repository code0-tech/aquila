use std::sync::{Arc};
use async_trait::async_trait;
use log::error;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisError};
use tokio::sync::Mutex;
use tucana_internal::sagittarius::{Flow};

pub struct FlowServiceBase {
    redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
}

#[async_trait]
pub trait FlowService {
    async fn new(redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> FlowServiceBase;
    async fn insert_flow(&mut self, flow: Flow);
    async fn insert_flows(&mut self, flows: Vec<Flow>);
    async fn delete_flow(&mut self, flow_id: i64);
    async fn delete_flows(&mut self, flow_ids: Vec<i64>);
    async fn get_all_flow_ids(&mut self) -> Result<Vec<String>, RedisError>;
}

#[async_trait]
impl FlowService for FlowServiceBase {
    
    async fn new(redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> FlowServiceBase {
        FlowServiceBase { redis_client_arc }
    }

    async fn insert_flow(&mut self, flow: Flow) {
        let mut connection = self.redis_client_arc.lock().await;
        let serialized_flow = serde_json::to_string(&flow).expect("");
        connection.set::<String, String, ()>(flow.flow_id.to_string(), serialized_flow).await.expect("Aga");
    }

    async fn insert_flows(&mut self, flows: Vec<Flow>) {
        let mut connection = self.redis_client_arc.lock().await;

        for flow in flows {
            let serialized_flow = serde_json::to_string(&flow);

            match serialized_flow {
                Ok(parsed_flow) => {
                    connection.set::<String, String, i64>(flow.flow_id.to_string(), parsed_flow);
                }
                Err(parse_error) => {
                    error!("Can't parse {} Because: {}", flow.flow_id, parse_error);
                    continue
                }
            }
        }
    }

    async fn delete_flow(&mut self, flow_id: i64) {
        let mut connection = self.redis_client_arc.lock().await;
        connection.del::<i64, i64>(flow_id);
    }

    async fn delete_flows(&mut self, flow_ids: Vec<i64>) {
        let mut connection = self.redis_client_arc.lock().await;
        connection.del::<Vec<i64>, i64>(flow_ids);
    }

    
    async fn get_all_flow_ids(&mut self) -> Result<Vec<String>, RedisError> {
        let mut connection = self.redis_client_arc.lock().await;

        match connection.keys("*").await {
            Ok(res) => Ok(res),
            Err(error) => Err(error)
        }
    }
}