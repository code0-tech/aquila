use futures::{StreamExt, TryStreamExt};
use prost::Message;
use std::sync::Arc;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius::{
    FlowLogonRequest, FlowResponse, flow_response::Data, flow_service_client::FlowServiceClient,
};

use crate::{authorization::authorization::get_authorization_metadata, flow::get_flow_identifier};

#[derive(Clone)]
pub struct SagittariusFlowClient {
    store: Arc<async_nats::jetstream::kv::Store>,
    client: FlowServiceClient<Channel>,
    token: String,
}

impl SagittariusFlowClient {
    pub async fn new(
        sagittarius_url: String,
        store: Arc<async_nats::jetstream::kv::Store>,
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
            store,
            client,
            token,
        }
    }

    async fn handle_response(&mut self, response: FlowResponse) {
        let data = match response.data {
            Some(data) => {
                log::info!("Received a FlowResponse");
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
                let identifier = format!("{}::*", id);
                match self.store.delete(identifier).await {
                    Ok(_) => log::info!("Flow deleted successfully"),
                    Err(err) => log::error!("Failed to delete flow. Reason: {:?}", err),
                };
            }
            //Will update the flow it receives
            Data::UpdatedFlow(flow) => {
                log::info!("Updating the Flow with the id: {}", &flow.flow_id);
                let key = get_flow_identifier(&flow);
                let bytes = flow.encode_to_vec();
                match self.store.put(key, bytes.into()).await {
                    Ok(_) => log::info!("Flow updated successfully"),
                    Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
                };
            }
            //WIll drop all flows that it holds and insert all new ones
            Data::Flows(flows) => {
                log::info!("Dropping all Flows & inserting the new ones!");
                let mut keys = match self.store.keys().await {
                    Ok(keys) => keys.boxed(),
                    Err(err) => {
                        log::error!("Service wasn't able to get keys. Reason: {:?}", err);
                        return;
                    }
                };

                let mut purged_count = 0;
                while let Ok(Some(key)) = keys.try_next().await {
                    match self.store.purge(&key).await {
                        Ok(_) => {
                            purged_count += 1;
                        }
                        Err(e) => log::error!("Failed to purge key {}: {}", key, e),
                    }
                }

                log::info!("Purged {} existing keys", purged_count);

                for flow in flows.flows {
                    let key = get_flow_identifier(&flow);
                    let bytes = flow.encode_to_vec();
                    match self.store.put(key, bytes.into()).await {
                        Ok(_) => log::info!("Flow updated successfully"),
                        Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
                    };
                }
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
                log::info!("Successfully established a Stream (for Flows)");
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
