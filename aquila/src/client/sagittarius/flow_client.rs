use aquila_store::{FlowService, FlowServiceBase};
use async_trait::async_trait;
use futures::StreamExt;
use log::{error, info};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tonic::Request;
use tucana::sagittarius::flow_service_client::FlowServiceClient;
use tucana::sagittarius::{FlowCommandType, FlowGetRequest, FlowLogonRequest, FlowResponse};

const INSERT: i32 = FlowCommandType::Insert as i32;
const DELETE: i32 = FlowCommandType::Delete as i32;

/// Struct representing a service for receiving flows from `Sagittarius`.
#[derive(Clone)]
pub struct SagittariusFlowClientBase {
    flow_service: Arc<Mutex<FlowServiceBase>>,
    client: FlowServiceClient<Channel>,
}

/// Trait representing a service for receiving flows from `Sagittarius`.
#[async_trait]
pub trait SagittariusFlowClient {
    async fn new(
        sagittarius_url: String,
        flow_service: Arc<Mutex<FlowServiceBase>>,
    ) -> SagittariusFlowClientBase;
    async fn send_flow_update_request(&mut self);
    async fn send_start_request(&mut self);
}

/// Implementation for a service for receiving flows from `Sagittarius`.
/// gRPC Service Implementation
#[async_trait]
impl SagittariusFlowClient for SagittariusFlowClientBase {
    /// Creates a connection to `Sagittarius`
    ///
    /// Behavior:
    /// Will panic when a connection can`t be established
    async fn new(
        sagittarius_url: String,
        flow_service: Arc<Mutex<FlowServiceBase>>,
    ) -> SagittariusFlowClientBase {
        let client = match FlowServiceClient::connect(sagittarius_url).await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {}", start_error);
            }
        };

        SagittariusFlowClientBase {
            flow_service,
            client,
        }
    }

    /// Will send a request `FlowGetRequest` to `Sagittarius`
    /// Inserts/Deletes flows contained in the response into Redis
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

    /// Will send a request `FlowLogonRequest` to `Sagittarius`
    /// Will establish a stream.
    /// `Sagittarius` will send update/delete commands and the flow to do that with.
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

        async fn handle_response(
            response: FlowResponse,
            flow_service: Arc<Mutex<FlowServiceBase>>,
        ) {
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
    //TODO: rewrite tests :(
}
