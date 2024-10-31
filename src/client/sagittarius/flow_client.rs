use std::sync::Arc;
use async_trait::async_trait;
use futures::StreamExt;
use log::{error, info};
use redis::AsyncCommands;
use tokio::sync::Mutex;
use tonic::Request;
use tonic::transport::Channel;
use tucana_internal::sagittarius::flow_service_client::FlowServiceClient;
use tucana_internal::sagittarius::{Flow, FlowCommandType, FlowGetRequest, FlowGetResponse, FlowLogonRequest, FlowResponse};
use crate::service::flow_service::{FlowService, FlowServiceBase};

const INSERT: i32 = FlowCommandType::Insert as i32;
const DELETE: i32 = FlowCommandType::Delete as i32;

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
            let mut flow_service = flow_service.lock().await;

            match response.r#type {
                INSERT => {
                    let flow = response.updated_flow;
                    if flow.is_none() {
                        info!("Recieved insert request without any flow");
                        return;
                    }

                    flow_service.insert_flow(flow.unwrap()).await;
                }
                DELETE => {
                    let flow = response.updated_flow;
                    if flow.is_none() {
                        info!("Recieved delete request without any flow");
                        return;
                    }

                    flow_service.insert_flow(flow.unwrap()).await;
                }
                _ => {
                    error!("Recieved unkown respone type")
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