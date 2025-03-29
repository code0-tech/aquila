use code0_flow::flow_store::service::{FlowStoreService, FlowStoreServiceBase};
use futures::StreamExt;
use log::error;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{async_trait, transport::Channel, Extensions, Request};
use tucana::sagittarius::{
    flow_response::Data, flow_service_client::FlowServiceClient, FlowLogonRequest, FlowResponse,
};

use crate::authorization::authorization::get_authorization_metadata;

#[derive(Clone)]
pub struct SagittariusFlowClient {
    flow_service: Arc<Mutex<FlowStoreService>>,
    client: FlowServiceClient<Channel>,
    token: String,
}

/// Trait representing a service for receiving flows from `Sagittarius`.
#[async_trait]
pub trait SagittariusServiceClient {
    async fn new(
        sagittarius_url: String,
        flow_service: Arc<Mutex<FlowStoreService>>,
        token: String,
    ) -> SagittariusFlowClient;
    async fn handle_response(&mut self, response: FlowResponse);
    async fn init_flow_stream(&mut self);
}

#[async_trait]
impl SagittariusServiceClient for SagittariusFlowClient {
    async fn new(
        sagittarius_url: String,
        flow_service: Arc<Mutex<FlowStoreService>>,
        token: String,
    ) -> SagittariusFlowClient {
        let client = match FlowServiceClient::connect(sagittarius_url).await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {}", start_error);
            }
        };

        SagittariusFlowClient {
            flow_service,
            client,
            token,
        }
    }

    async fn handle_response(&mut self, response: FlowResponse) {
        let data = match response.data {
            Some(data) => data,
            None => {
                print!("Received a FlowLogonResponse but no FlowLogonResponse");
                return;
            }
        };

        match data {
            // Will delete the flow id it receives
            Data::DeletedFlowId(id) => {
                let mut flow_service_lock = self.flow_service.lock().await;
                let _ = flow_service_lock.delete_flow(id).await;
            }
            //Will update the flow it receives
            Data::UpdatedFlow(flow) => {
                let mut flow_service_lock = self.flow_service.lock().await;
                let _ = flow_service_lock.insert_flow(flow).await;
            }
            //WIll drop all flows that it holds and insert all new ones
            Data::Flows(flows) => {
                let mut flow_service_lock = self.flow_service.lock().await;
                let result_ids = flow_service_lock.get_all_flow_ids().await;

                let ids = match result_ids {
                    Ok(ids) => ids,
                    Err(err) => {
                        error!("Service wasn't able to get ids {}", err);
                        return;
                    }
                };

                let _ = flow_service_lock.delete_flows(ids).await;
                let _ = flow_service_lock.insert_flows(flows).await;
            }
        }
    }

    async fn init_flow_stream(&mut self) {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            FlowLogonRequest {},
        );

        let response = match self.client.update(request).await {
            Ok(res) => res,
            Err(status) => {
                panic!("Received a {status}, can't retrieve flows from Sagittarius");
            }
        };

        let mut stream = response.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(res) => {
                    self.handle_response(res).await;
                }
                Err(status) => {
                    panic!("Received a {status}, can't retrieve flows from Sagittarius");
                }
            };
        }
    }
}
