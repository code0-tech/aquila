use code0_flow::flow_store::service::{FlowStoreService, FlowStoreServiceBase};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{transport::Channel, Extensions, Request};
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

impl SagittariusFlowClient {
    pub async fn new(
        sagittarius_url: String,
        flow_service: Arc<Mutex<FlowStoreService>>,
        token: String,
    ) -> SagittariusFlowClient {
        let client = match FlowServiceClient::connect(sagittarius_url).await {
            Ok(res) => {
                log::info!("Successfully connected to Sagittarius Flow Endpoint!");
                res
            }
            Err(err) => panic!(
                "Failed to connect to Sagittarius (Flow Endpoint): {:?}",
                err
            ),
        };

        SagittariusFlowClient {
            flow_service,
            client,
            token,
        }
    }

    async fn handle_response(&mut self, response: FlowResponse) {
        let data = match response.data {
            Some(data) => {
                log::info!("Recieved a FlowResponse");
                data
            }
            None => {
                log::error!("Received a empty FlowResponse");
                return;
            }
        };

        match data {
            // Will delete the flow id it receives
            Data::DeletedFlowId(id) => {
                log::info!("Deleting the Flow with the id: {}", id);
                let mut flow_service_lock = self.flow_service.lock().await;
                match flow_service_lock.delete_flow(id).await {
                    Ok(_) => log::info!("Flow deleted successfully"),
                    Err(err) => log::error!("Failed to delete flow. Reason: {:?}", err),
                };
            }
            //Will update the flow it receives
            Data::UpdatedFlow(flow) => {
                log::info!("Updating the Flow with the id: {}", &flow.flow_id);
                let mut flow_service_lock = self.flow_service.lock().await;
                match flow_service_lock.insert_flow(flow).await {
                    Ok(_) => log::info!("Flow updated successfully"),
                    Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
                };
            }
            //WIll drop all flows that it holds and insert all new ones
            Data::Flows(flows) => {
                log::info!("Dropping all Flows & inserting the new ones!");
                let mut flow_service_lock = self.flow_service.lock().await;
                let result_ids = flow_service_lock.get_all_flow_ids().await;

                let ids = match result_ids {
                    Ok(ids) => ids,
                    Err(err) => {
                        log::error!("Service wasn't able to get ids. Reason: {:?}", err);
                        return;
                    }
                };

                match flow_service_lock.delete_flows(ids).await {
                    Ok(amount) => {
                        log::info!("Deleted {} flows", amount);
                    }
                    Err(err) => {
                        log::error!("Service wasn't able to delete flows. Reason: {:?}", err);
                    }
                };
                match flow_service_lock.insert_flows(flows).await {
                    Ok(amount) => {
                        log::info!("Inserted {} flows", amount);
                    }
                    Err(err) => {
                        log::error!("Service wasn't able to insert flows. Reason: {:?}", err);
                    }
                };
            }
        }
    }

    pub async fn init_flow_stream(&mut self) {
        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            FlowLogonRequest {},
        );

        let response = match self.client.update(request).await {
            Ok(res) => {
                log::info!("Succesfully established a Stream (for Flows)");
                res
            }
            Err(status) => {
                log::error!(
                    "Received a {:?}, can't retrieve flows from Sagittarius",
                    status
                );
                return;
            }
        };

        let mut stream = response.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(res) => {
                    self.handle_response(res).await;
                }
                Err(status) => {
                    log::error!(
                        "Received a {:?}, can't retrieve flows from Sagittarius",
                        status
                    );
                }
            };
        }
    }
}
