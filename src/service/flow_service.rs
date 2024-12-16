use async_trait::async_trait;
use log::{debug, error};
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, RedisError};
use std::sync::Arc;
use tokio::sync::Mutex;
use tucana::sagittarius::Flow;

/// Struct representing a service for managing flows in a Redis.
pub struct FlowServiceBase {
    pub(crate) redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
}

/// Trait representing a service for managing flows in a Redis.
#[async_trait]
pub trait FlowService {
    async fn new(redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> FlowServiceBase;
    async fn insert_flow(&mut self, flow: Flow);
    async fn insert_flows(&mut self, flows: Vec<Flow>);
    async fn delete_flow(&mut self, flow_id: i64);
    async fn delete_flows(&mut self, flow_ids: Vec<i64>);
    async fn get_all_flow_ids(&mut self) -> Result<Vec<i64>, RedisError>;
}

/// Implementation of a service for managing flows in a Redis.
#[async_trait]
impl FlowService for FlowServiceBase {
    async fn new(redis_client_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> FlowServiceBase {
        FlowServiceBase { redis_client_arc }
    }

    /// Insert a list of flows into Redis
    async fn insert_flow(&mut self, flow: Flow) {
        let mut connection = self.redis_client_arc.lock().await;

        let serialized_flow = match serde_json::to_string(&flow) {
            Ok(serialized_flow) => serialized_flow,
            Err(parse_error) => {
                error!("An Error occurred {}", parse_error);
                return;
            }
        };

        let parsed_flow = connection
            .set::<String, String, i64>(flow.flow_id.to_string(), serialized_flow)
            .await;

        match parsed_flow {
            Ok(_) => {
                debug!("Inserted flow");
            }
            Err(redis_error) => {
                error!("An Error occurred {}", redis_error);
            }
        }
    }

    /// Insert a flows into Redis
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

    /// Deletes a flow
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

    /// Deletes a list of flows
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

    /// Queries for all ids in the redis
    /// Returns `Result<Vec<i64>, RedisError>`: Result of the flow ids currently in Redis
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc};
    use redis::AsyncCommands;
    use tokio::sync::Mutex;
    use tucana::sagittarius::Flow;
    use crate::data::redis::setup_redis_test_container;
    use crate::service::flow_service::{FlowService, FlowServiceBase};

    #[tokio::test]
    async fn test_get_all_flow_ids_redis_error() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        drop(_container);

        let flow_ids = service.get_all_flow_ids().await;
        assert!(flow_ids.is_err(), "Expected an error due to Redis disconnection");
    }

    #[tokio::test]
    async fn test_insert_flow_once() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        service.insert_flow(flow.clone()).await;

        let result: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result.is_some());

        let decoded_flow: Flow = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(decoded_flow.flow_id, flow.flow_id);

        let flow_ids = service.get_all_flow_ids().await;
        assert!(flow_ids.is_ok());
        assert_eq!(flow_ids.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_insert_flow_with_same_id_will_overwrite() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        service.insert_flow(flow.clone()).await;

        let result: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result.is_some());

        let decoded_flow: Flow = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(decoded_flow.flow_id, flow.flow_id);

        service.insert_flow(flow.clone()).await;
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_insert_flows_once() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow_1 = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        let flow_2 = Flow {
            flow_id: 2,
            start_node: None,
            definition: None,
        };

        let flows = vec![flow_1.clone(), flow_2.clone()];

        service.insert_flows(flows).await;

        let results: (Option<String>, Option<String>) = {
            let mut conn = redis_client.lock().await;
            (conn.get("1").await.unwrap(), conn.get("2").await.unwrap())
        };

        assert!(results.0.is_some());
        assert!(results.1.is_some());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_insert_flows_empty() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;
        let flows: Vec<Flow> = vec![];

        service.insert_flows(flows).await;
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_insert_flows_with_duplicate_id() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow_1 = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        let flow_2 = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        let flows = vec![flow_1.clone(), flow_2.clone()];

        service.insert_flows(flows).await;

        let result: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result.is_some());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_get_all_flow_ids_empty() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow_ids = service.get_all_flow_ids().await;

        assert!(flow_ids.is_ok());
        assert!(flow_ids.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_delete_exising_flow() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        service.insert_flow(flow.clone()).await;
        let result: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result.is_some());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 1);

        service.delete_flow(1).await;

        let result_after: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result_after.is_none());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_non_existing_flow_does_not_crash() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let result_after: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };
        assert!(result_after.is_none());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 0);

        service.delete_flow(1).await;
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_existing_flow() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow_1 = Flow {
            flow_id: 1,
            start_node: None,
            definition: None,
        };

        let flow_2 = Flow {
            flow_id: 2,
            start_node: None,
            definition: None,
        };

        let flows = vec![flow_1.clone(), flow_2.clone()];

        service.insert_flows(flows).await;

        let results: (Option<String>, Option<String>) = {
            let mut conn = redis_client.lock().await;
            (conn.get("1").await.unwrap(), conn.get("2").await.unwrap())
        };

        assert!(results.0.is_some());
        assert!(results.1.is_some());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 2);

        service.delete_flows(vec![1, 2]).await;
        let results_after: (Option<String>, Option<String>) = {
            let mut conn = redis_client.lock().await;
            (conn.get("1").await.unwrap(), conn.get("2").await.unwrap())
        };

        assert!(results_after.0.is_none());
        assert!(results_after.1.is_none());
        assert_eq!(service.get_all_flow_ids().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_delete_flows_mixed_ids() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow_1 = Flow { flow_id: 1, start_node: None, definition: None };
        let flow_2 = Flow { flow_id: 2, start_node: None, definition: None };
        service.insert_flows(vec![flow_1.clone(), flow_2.clone()]).await;

        service.delete_flows(vec![1, 3]).await;

        let result_1: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };
        let result_2: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("2").await.unwrap()
        };

        assert!(result_1.is_none(), "Flow with ID 1 should be deleted");
        assert!(result_2.is_some(), "Flow with ID 2 should still exist");
    }

    #[tokio::test]
    async fn test_delete_flows_empty_list() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let mut service = FlowServiceBase::new(redis_client.clone()).await;

        let flow = Flow { flow_id: 1, start_node: None, definition: None };
        service.insert_flow(flow.clone()).await;

        service.delete_flows(vec![]).await;

        let result: Option<String> = {
            let mut conn = redis_client.lock().await;
            conn.get("1").await.unwrap()
        };

        assert!(result.is_some());
    }
}