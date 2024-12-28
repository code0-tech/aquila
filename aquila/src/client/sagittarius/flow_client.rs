use aquila_store::{FlowService, FlowServiceBase};
use async_trait::async_trait;
use futures::StreamExt;
use log::{debug, error};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::transport::Channel;
use tonic::Request;
use tucana::sagittarius::flow_response::Data;
use tucana::sagittarius::flow_service_client::FlowServiceClient;
use tucana::sagittarius::{FlowLogonRequest, FlowResponse};

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
    async fn init_flow_stream(&mut self);
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

    /// Will send a request `FlowLogonRequest` to `Sagittarius`
    /// Will establish a stream.
    /// `Sagittarius` will send update/delete commands and the flow to do that with.
    async fn init_flow_stream(&mut self) {
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
            let data = match response.data {
                Some(data) => data,
                None => {
                    debug!("Received a FlowLogonResponse but no FlowLogonResponse");
                    return;
                }
            };

            match data {
                // Will delete the flow id it receives
                Data::DeletedFlowId(id) => {
                    let mut flow_service = flow_service.lock().await;
                    flow_service.delete_flow(id).await;
                }
                //Will update the flow it receives
                Data::UpdatedFlow(flow) => {
                    let mut flow_service = flow_service.lock().await;
                    flow_service.insert_flow(flow).await;
                }
                //WIll drop all flows that it holds and insert all new ones
                Data::Flows(flows) => {
                    let mut flow_service = flow_service.lock().await;
                    let result_ids = flow_service.get_all_flow_ids().await;

                    let ids = match result_ids {
                        Ok(ids) => ids,
                        Err(err) => {
                            error!("Service wasn't able to get ids {}", err);
                            return;
                        }
                    };

                    flow_service.delete_flows(ids).await;
                    flow_service.insert_flows(flows.flows).await;
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
