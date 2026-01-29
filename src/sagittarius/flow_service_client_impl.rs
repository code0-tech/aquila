use futures::{StreamExt, TryStreamExt};
use prost::Message;
use tokio::fs;
use std::{path::Path, sync::Arc};
use tonic::{Extensions, Request, transport::Channel};
use tucana::{sagittarius::{
    FlowLogonRequest, FlowResponse, flow_response::Data, flow_service_client::FlowServiceClient,
}, shared::{Flows, ValidationFlow}};

use crate::{authorization::authorization::get_authorization_metadata, flow::get_flow_identifier};

#[derive(Clone)]
pub struct SagittariusFlowClient {
    store: Arc<async_nats::jetstream::kv::Store>,
    client: FlowServiceClient<Channel>,
    env: String,
    token: String,
}

impl SagittariusFlowClient {
    pub async fn new(
        sagittarius_url: String,
        store: Arc<async_nats::jetstream::kv::Store>,
        env: String,
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
            env,
            token,
        }
    }

    fn is_development(&self) -> bool {
        self.env == String::from("DEVELOPMENT")
    }

    async fn export_flows_json_overwrite(&self, flows: Flows) {
        if !self.is_development() {
            return;
        }

        log::info!("Will export flows into file because env is set to `DEVELOPMENT`");

        let json = match serde_json::to_vec_pretty(&flows) {
            Ok(b) => b,
            Err(e) => {
                log::error!("Failed to serialize flows to JSON: {:?}", e);
                return;
            }
        };

        let final_path = Path::new("flowExport.json");
        let tmp_path = Path::new("flowExport.json.tmp");

        if let Err(e) = fs::write(tmp_path, &json).await {
            log::error!("Failed to write {}: {}", tmp_path.display(), e);
            return;
        }

        if let Err(e) = fs::rename(tmp_path, final_path).await {
            log::warn!("rename failed (will try remove+rename): {}", e);
            let _ = fs::remove_file(final_path).await;
            if let Err(e2) = fs::rename(tmp_path, final_path).await {
                log::error!("Failed to move export into place: {}", e2);
                let _ = fs::remove_file(tmp_path).await;
            }
        }

        log::info!("Exported {} flows to {}", flows.flows.len(), final_path.display());
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
                
                // Writing all flows into an output if its in `DEVELOPMENT`
                self.export_flows_json_overwrite(flows.clone()).await;

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
                    log::debug!("trying to insert: {}", key);
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
