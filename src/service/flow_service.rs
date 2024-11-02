use async_trait::async_trait;
use futures::future::err;
use log::{debug, error};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisError};
use std::sync::Arc;
use tokio::sync::Mutex;
use tucana_internal::sagittarius::Flow;

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
    async fn get_all_flow_ids(&mut self) -> Result<Vec<i64>, RedisError>;
}

#[async_trait]
impl FlowService for FlowServiceBase {
    async fn new(redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> FlowServiceBase {
        FlowServiceBase { redis_client_arc }
    }

    async fn insert_flow(&mut self, flow: Flow) {
        let mut connection = self.redis_client_arc.lock().await;
        let serialized_flow = serde_json::to_string(&flow).expect("");
        let parsed_flow = connection.set::<String, String, ()>(flow.flow_id.to_string(), serialized_flow).await;

        match parsed_flow {
            Ok(_) => {
                debug!("Inserted flow");
            }
            Err(redis_error) => {
                error!("An Error occurred {}", redis_error);
            }
        }
    }

    async fn insert_flows(&mut self, flows: Vec<Flow>) {
        let mut connection = self.redis_client_arc.lock().await;

        for flow in flows {
            let serialized_flow = serde_json::to_string(&flow);

            let parsed_flow = match serialized_flow {
                Ok(parsed_flow) => {
                    connection.set::<String, String, i64>(flow.flow_id.to_string(), parsed_flow).await
                }
                Err(parse_error) => {
                    error!("Can't parse {} Because: {}", flow.flow_id, parse_error);
                    continue;
                }
            };

            match parsed_flow {
                Ok(_) => {
                    debug!("Inserted flow");
                }
                Err(redis_error) => {
                    error!("An Error occurred {}", redis_error);
                }
            }
        }
    }

    async fn delete_flow(&mut self, flow_id: i64) {
        let mut connection = self.redis_client_arc.lock().await;
        let deleted_flow = connection.del::<i64, i64>(flow_id).await;

        match deleted_flow {
            Ok(changed_amount) => {
                debug!("{} flows where deleted", changed_amount);
            }
            Err(redis_error) => {
                error!("An Error occurred {}", redis_error);
            }
        }
    }

    async fn delete_flows(&mut self, flow_ids: Vec<i64>) {
        let mut connection = self.redis_client_arc.lock().await;
        let deleted_flow = connection.del::<Vec<i64>, i64>(flow_ids).await;

        match deleted_flow {
            Ok(changed_amount) => {
                debug!("{} flows where deleted", changed_amount);
            }
            Err(redis_error) => {
                error!("An Error occurred {}", redis_error);
            }
        }
    }


    async fn get_all_flow_ids(&mut self) -> Result<Vec<i64>, RedisError> {
        let mut connection = self.redis_client_arc.lock().await;

        let string_keys: Vec<String> = {
            match connection.keys("*").await {
                Ok(res) => res,
                Err(error) => {
                    print!("Can't retrieve keys from redis. Reason: {error}");
                    return Err(error);
                }
            }
        };

        let int_keys: Vec<i64> = string_keys
            .into_iter()
            .filter_map(|key| key.parse::<i64>().ok())
            .collect();

        Ok(int_keys)
    }
}

mod test {

    use std::sync::{Arc};
    use redis::AsyncCommands;
    use tokio::sync::Mutex;
    use tucana_internal::sagittarius::Flow;
    use crate::data::redis::setup_redis_test_container;
    use crate::service::flow_service::{FlowService, FlowServiceBase};

    #[tokio::test]
    async fn test_insert_flow_positive() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        service.insert_flow(flow.clone()).await;

        let mut conn = redis_client.lock().await;
        let result: Option<String> = conn.get("1").await.unwrap();
        assert!(result.is_some());

        let decoded_flow: Flow = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(decoded_flow.flow_id, flow.flow_id);
    }
}