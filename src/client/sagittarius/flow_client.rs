use crate::service::flow_service::{FlowService, FlowServiceBase};
use async_trait::async_trait;
use futures::StreamExt;
use log::{error, info};
use redis::AsyncCommands;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tonic::Request;
use tucana_internal::sagittarius::flow_service_client::FlowServiceClient;
use tucana_internal::sagittarius::{FlowCommandType, FlowGetRequest, FlowLogonRequest, FlowResponse};

const INSERT: i32 = FlowCommandType::Insert as i32;
const DELETE: i32 = FlowCommandType::Delete as i32;

#[derive(Clone)]
pub struct SagittariusFlowClientBase {
    flow_service: Arc<Mutex<FlowServiceBase>>,
    client: FlowServiceClient<Channel>,
}

#[async_trait]
pub trait SagittariusFlowClient {
    async fn new(sagittarius_url: String, flow_service: Arc<Mutex<FlowServiceBase>>) -> SagittariusFlowClientBase;
    async fn send_flow_update_request(&mut self);
    async fn send_start_request(&mut self);
}

#[async_trait]
impl SagittariusFlowClient for SagittariusFlowClientBase {
    async fn new(sagittarius_url: String, flow_service: Arc<Mutex<FlowServiceBase>>) -> SagittariusFlowClientBase {
        let client = match FlowServiceClient::connect(sagittarius_url).await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {}", start_error);
            }
        };

        SagittariusFlowClientBase { flow_service, client }
    }

    async fn send_flow_update_request(&mut self) {
        let mut flow_service = self.flow_service.lock().await;
        let flow_ids = match flow_service.get_all_flow_ids().await {
            Ok(result) => result,
            Err(redis_error) => {
                error!("Service wasn't able to get ids {}", redis_error);
                return;
            }
        };

        let request = Request::new(FlowGetRequest { flow_ids });

        let response = match self.client.get(request).await {
            Ok(res) => res.into_inner(),
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
                return;
            }
        };

        let update_flows = response.updated_flows;
        let deleted_flow_ids = response.deleted_flow_ids;
        flow_service.insert_flows(update_flows).await;
        flow_service.delete_flows(deleted_flow_ids).await
    }

    async fn send_start_request(&mut self) {
        let request = Request::new(FlowLogonRequest {});
        let response = match self.client.update(request).await {
            Ok(res) => res,
            Err(status) => {
                error!("Received a {status}, can't retrieve flows from Sagittarius");
                return;
            }
        };

        let mut stream = response.into_inner();

        async fn handle_response(response: FlowResponse, flow_service: Arc<Mutex<FlowServiceBase>>) {
            match response.r#type {
                INSERT => {
                    let flow = response.updated_flow;
                    if flow.is_none() {
                        info!("Received insert request without any flow");
                        return;
                    }

                    {
                        let mut flow_service = flow_service.lock().await;
                        flow_service.insert_flow(flow.unwrap()).await;
                    }
                }
                DELETE => {
                    let flow_id = response.deleted_flow_id;
                    if flow_id.is_none() {
                        info!("Received delete request without any flow");
                        return;
                    }

                    {
                        let mut flow_service = flow_service.lock().await;
                        flow_service.delete_flow(flow_id.unwrap()).await;
                    }
                }
                _ => {
                    error!("Received unknown response type")
                }
            }
        }

        while let Some(result) = stream.next().await {
            match result {
                Ok(res) => {
                    handle_response(res, self.flow_service.clone()).await;
                }
                Err(status) => {
                    error!("Received a {status}, can't retrieve flows from Sagittarius");
                    return;
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::client::sagittarius::action_client::{SagittariusActionClient, SagittariusActionClientBase};
    use crate::client::sagittarius::flow_client::{SagittariusFlowClient, SagittariusFlowClientBase};
    use crate::data::redis::setup_redis_test_container;
    use crate::service::flow_service::FlowService;
    use crate::service::flow_service::FlowServiceBase;
    use async_trait::async_trait;
    use std::pin::Pin;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::{oneshot, Mutex};
    use tokio::task::JoinHandle;
    use tonic::codegen::tokio_stream::wrappers::TcpListenerStream;
    use tonic::codegen::tokio_stream::Stream;
    use tonic::transport::Server;
    use tonic::{Request, Response, Status};
    use tucana_internal::sagittarius::flow_service_server::{FlowService as SagittariusFlowService, FlowServiceServer};
    use tucana_internal::sagittarius::{Flow, FlowGetRequest, FlowGetResponse, FlowLogonRequest, FlowResponse};

    struct MockFlowService {
        flow_get_result: FlowGetResponse,
    }

    #[derive(Default)]
    struct BrokenMockFlowService;

    impl MockFlowService {
        pub fn new(flow_get_result: FlowGetResponse) -> Self {
            MockFlowService { flow_get_result }
        }
    }

    #[async_trait]
    impl SagittariusFlowService for MockFlowService {
        async fn get(&self, _request: Request<FlowGetRequest>) -> Result<Response<FlowGetResponse>, Status> {
            Ok(Response::new(self.flow_get_result.clone()))
        }

        type UpdateStream = Pin<Box<dyn Stream<Item=Result<FlowResponse, Status>> + Send>>;

        async fn update(&self, _request: Request<FlowLogonRequest>) -> Result<Response<Self::UpdateStream>, Status> {
            let flow = Flow {
                flow_id: 1,
                start_node: None,
                definition: None,
            };

            let response_stream = async_stream::try_stream! {
                yield FlowResponse {
                    updated_flow: Some(flow),
                    deleted_flow_id: None,
                    r#type: 0,
                };
            };

            Ok(Response::new(Box::pin(response_stream) as Self::UpdateStream))
        }
    }

    #[async_trait]
    impl SagittariusFlowService for BrokenMockFlowService {
        async fn get(&self, _request: Request<FlowGetRequest>) -> Result<Response<FlowGetResponse>, Status> {
            Err(Status::internal("An unhandled error occurred!"))
        }

        type UpdateStream = Pin<Box<dyn Stream<Item=Result<FlowResponse, Status>> + Send>>;

        async fn update(&self, _request: Request<FlowLogonRequest>) -> Result<Response<Self::UpdateStream>, Status> {
            Err(Status::internal("An unhandled error occurred!"))
        }
    }

    async fn setup_sagittarius_mock(flow_get_response: FlowGetResponse) -> (JoinHandle<()>, oneshot::Sender<()>, String) {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);

        let mock_service = MockFlowService::new(flow_get_response);

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(FlowServiceServer::new(mock_service))
                .serve_with_incoming_shutdown(incoming, async {
                    shutdown_rx.await.ok();
                })
                .await
                .unwrap();
        });

        (server_handle, shutdown_tx, format!("http://{}", addr))
    }

    async fn setup_broken_sagittarius_mock() -> (JoinHandle<()>, oneshot::Sender<()>, String) {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = TcpListenerStream::new(listener);

        let mock_service = BrokenMockFlowService::default();

        let server_handle = tokio::spawn(async move {
            Server::builder()
                .add_service(FlowServiceServer::new(mock_service))
                .serve_with_incoming_shutdown(incoming, async {
                    shutdown_rx.await.ok();
                })
                .await
                .unwrap();
        });

        (server_handle, shutdown_tx, format!("http://{}", addr))
    }

    #[tokio::test]
    async fn test_get_flow_insert_successfully() {
        let response = FlowGetResponse {
            updated_flows: vec![],
            deleted_flow_ids: vec![],
        };

        let (connection, _container) = setup_redis_test_container().await;
        let (server_handle, shutdown, url) = setup_sagittarius_mock(response).await;

        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let service = FlowServiceBase::new(redis_client.clone()).await;
        let service_arc = Arc::new(Mutex::new(service));

        let mut client = SagittariusFlowClientBase::new(url.clone(), service_arc.clone()).await;
        client.send_start_request().await;

        let data_after = {
            let mut current_service = service_arc.lock().await;
            current_service.get_all_flow_ids().await
        };

        assert!(data_after.is_ok());
        assert_eq!(data_after.unwrap().len(), 1);

        shutdown.send(()).expect("Failed to send shutdown signal");
        server_handle.await.expect("Failed to await server handle");
    }

    #[tokio::test]
    async fn test_delete_flows_empty_list_not_crash() {
        let (connection, _container) = setup_redis_test_container().await;
        let (_server_handle, shutdown, url) = setup_broken_sagittarius_mock().await;

        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let service = FlowServiceBase::new(redis_client.clone()).await;
        let service_arc = Arc::new(Mutex::new(service));

        let mut client = SagittariusFlowClientBase::new(url, service_arc.clone()).await;
        client.send_start_request().await;

        shutdown.send(()).expect("Failed to send shutdown signal");
    }

    #[tokio::test]
    async fn test_get_flows_update_only() {
        let response = FlowGetResponse {
            updated_flows: vec![
                Flow { flow_id: 1, start_node: None, definition: None },
                Flow { flow_id: 2, start_node: None, definition: None },
                Flow { flow_id: 3, start_node: None, definition: None },
            ],
            deleted_flow_ids: vec![],
        };

        let (connection, _container) = setup_redis_test_container().await;
        let (server_handle, shutdown, url) = setup_sagittarius_mock(response).await;

        let redis_client = Arc::new(Mutex::new(Box::new(connection)));
        let service = FlowServiceBase::new(redis_client.clone()).await;
        let service_arc = Arc::new(Mutex::new(service));

        let mut client = SagittariusFlowClientBase::new(url.clone(), service_arc.clone()).await;
        client.send_flow_update_request().await;

        let data_after = {
            let mut current_service = service_arc.lock().await;
            current_service.get_all_flow_ids().await
        };

        assert!(data_after.is_ok());
        assert_eq!(data_after.unwrap().len(), 3);

        shutdown.send(()).expect("Failed to send shutdown signal");
        server_handle.await.expect("Failed to await server handle");
    }

    #[tokio::test]
    async fn test_get_flows_update_and_delete() {
        let (connection, _container) = setup_redis_test_container().await;
        let redis_client = Arc::new(Mutex::new(Box::new(connection)));

        {
            let response = FlowGetResponse {
                updated_flows: vec![
                    Flow { flow_id: 1, start_node: None, definition: None },
                    Flow { flow_id: 2, start_node: None, definition: None },
                    Flow { flow_id: 3, start_node: None, definition: None },
                ],
                deleted_flow_ids: vec![],
            };

            let (server_handle, shutdown, url) = setup_sagittarius_mock(response).await;
            let service = FlowServiceBase::new(redis_client.clone()).await;
            let service_arc = Arc::new(Mutex::new(service));

            let mut client = SagittariusFlowClientBase::new(url.clone(), service_arc.clone()).await;
            client.send_flow_update_request().await;

            let data_after = {
                let mut current_service = service_arc.lock().await;
                current_service.get_all_flow_ids().await
            };

            assert!(data_after.is_ok());
            assert_eq!(data_after.unwrap().len(), 3);

            shutdown.send(()).expect("Failed to send shutdown signal");
            server_handle.await.expect("Failed to await server handle");
        };

        {
            let response = FlowGetResponse {
                updated_flows: vec![],
                deleted_flow_ids: vec![1, 2],
            };

            let (server_handle, shutdown, url) = setup_sagittarius_mock(response).await;
            let service = FlowServiceBase::new(redis_client.clone()).await;
            let service_arc = Arc::new(Mutex::new(service));

            let mut client = SagittariusFlowClientBase::new(url.clone(), service_arc.clone()).await;
            client.send_flow_update_request().await;

            let data_after = {
                let mut current_service = service_arc.lock().await;
                current_service.get_all_flow_ids().await
            };

            assert!(data_after.is_ok());
            assert_eq!(data_after.unwrap().len(), 1);

            shutdown.send(()).expect("Failed to send shutdown signal");
            server_handle.await.expect("Failed to await server handle");
        };
    }

    #[tokio::test]
    #[should_panic(expected = "Can't start client")]
    async fn test_sagittarius_action_client_new_should_panic() {
        let sagittarius_url = "http://127.0.0.1:25565".to_string();
        let _client = SagittariusActionClientBase::new(sagittarius_url).await;
    }
}