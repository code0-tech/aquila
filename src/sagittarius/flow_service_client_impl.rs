use crate::{authorization::authorization::get_authorization_metadata, flow::get_flow_identifier};
use futures::{StreamExt, TryStreamExt};
use prost::Message;
use std::{path::Path, sync::Arc};
use tokio::fs;
use tokio::sync::broadcast;
use tonic::{Extensions, Request, transport::Channel};
use tucana::{
    sagittarius::{
        FlowLogonRequest, FlowResponse, flow_response::Data, flow_service_client::FlowServiceClient,
    },
    shared::Flows,
};

use std::sync::atomic::{AtomicBool, Ordering};

fn module_config_stats(configs: &tucana::shared::ModuleConfigurations) -> (usize, usize) {
    let project_count = configs.module_configurations.len();
    let config_count = configs
        .module_configurations
        .iter()
        .map(|project_cfg| project_cfg.module_configurations.len())
        .sum();

    (project_count, config_count)
}

fn key_has_flow_id(key: &str, flow_id: i64) -> bool {
    key.rsplit_once('.')
        .and_then(|(_, id)| id.parse::<i64>().ok())
        == Some(flow_id)
}

#[derive(Clone)]
pub struct SagittariusFlowClient {
    store: Arc<async_nats::jetstream::kv::Store>,
    client: FlowServiceClient<Channel>,
    env: String,
    token: String,
    sagittarius_ready: Arc<AtomicBool>,
    action_config_tx: broadcast::Sender<tucana::shared::ModuleConfigurations>,
}

impl SagittariusFlowClient {
    pub fn new(
        store: Arc<async_nats::jetstream::kv::Store>,
        env: String,
        token: String,
        channel: Channel,
        sagittarius_ready: Arc<AtomicBool>,
        action_config_tx: broadcast::Sender<tucana::shared::ModuleConfigurations>,
    ) -> SagittariusFlowClient {
        let client = FlowServiceClient::new(channel);

        SagittariusFlowClient {
            store,
            client,
            env,
            token,
            sagittarius_ready,
            action_config_tx,
        }
    }

    fn is_development(&self) -> bool {
        self.env == "DEVELOPMENT"
    }

    async fn export_flows_json_overwrite(&self, flows: Flows) {
        if !self.is_development() {
            return;
        }

        let json = match serde_json::to_vec_pretty(&flows) {
            Ok(b) => b,
            Err(e) => {
                log::error!(
                    "Failed to serialize development flow export flow_count={} error={:?}",
                    flows.flows.len(),
                    e
                );
                return;
            }
        };

        let final_path = Path::new("flowExport.json");
        let tmp_path = Path::new("flowExport.json.tmp");

        if let Err(e) = fs::write(tmp_path, &json).await {
            log::error!(
                "Failed to write development flow export path={} error={}",
                tmp_path.display(),
                e
            );
            return;
        }

        if let Err(e) = fs::rename(tmp_path, final_path).await {
            log::warn!(
                "Could not atomically replace development flow export path={} error={}; retrying after removing destination",
                final_path.display(),
                e
            );
            match fs::remove_file(final_path).await {
                Ok(()) => log::debug!(
                    "Removed previous development flow export path={}",
                    final_path.display()
                ),
                Err(remove_error) if remove_error.kind() == std::io::ErrorKind::NotFound => {}
                Err(remove_error) => log::warn!(
                    "Failed to remove previous development flow export path={} error={}",
                    final_path.display(),
                    remove_error
                ),
            }
            if let Err(e2) = fs::rename(tmp_path, final_path).await {
                log::error!(
                    "Failed to replace development flow export source_path={} destination_path={} initial_rename_error={} retry_error={}",
                    tmp_path.display(),
                    final_path.display(),
                    e,
                    e2
                );
                if let Err(cleanup_error) = fs::remove_file(tmp_path).await {
                    log::warn!(
                        "Failed to clean up temporary development flow export path={} error={}",
                        tmp_path.display(),
                        cleanup_error
                    );
                }
                return;
            }
        }

        log::info!(
            "Exported {} flows to {}",
            flows.flows.len(),
            final_path.display()
        );
    }

    async fn handle_response(&mut self, response: FlowResponse) {
        let data = match response.data {
            Some(data) => data,
            None => {
                log::warn!("Received empty Sagittarius flow response");
                return;
            }
        };

        match data {
            Data::DeletedFlowId(id) => {
                log::debug!("Applying flow deletion flow_id={}", id);
                let mut keys = match self.store.keys().await {
                    Ok(keys) => keys.boxed(),
                    Err(err) => {
                        log::error!(
                            "Failed to list stored flows for deletion flow_id={} error={:?}",
                            id,
                            err
                        );
                        return;
                    }
                };

                let mut deleted_count = 0;
                while let Ok(Some(key)) = keys.try_next().await {
                    if !key_has_flow_id(&key, id) {
                        continue;
                    }

                    match self.store.delete(&key).await {
                        Ok(_) => deleted_count += 1,
                        Err(err) => log::error!(
                            "Failed to delete stored flow flow_id={} key={} error={:?}",
                            id,
                            key,
                            err
                        ),
                    }
                }

                if deleted_count == 0 {
                    log::warn!("Flow deletion matched no stored keys flow_id={}", id);
                } else {
                    log::info!(
                        "Flow deleted successfully id={} deleted_keys={}",
                        id,
                        deleted_count
                    );
                }
            }
            Data::UpdatedFlow(flow) => {
                let key = get_flow_identifier(&flow);
                let flow_id = flow.flow_id.clone();
                let bytes = flow.encode_to_vec();
                match self.store.put(key.clone(), bytes.into()).await {
                    Ok(_) => log::info!("Stored flow update flow_id={} key={}", flow_id, key),
                    Err(err) => log::error!(
                        "Failed to store flow update flow_id={} key={} error={:?}",
                        flow_id,
                        key,
                        err
                    ),
                };
            }
            Data::Flows(flows) => {
                let received_count = flows.flows.len();
                log::info!(
                    "Replacing stored flows from Sagittarius received_count={}",
                    received_count
                );

                self.export_flows_json_overwrite(flows.clone()).await;

                let mut keys = match self.store.keys().await {
                    Ok(keys) => keys.boxed(),
                    Err(err) => {
                        log::error!(
                            "Failed to list stored flows before replacement error={:?}",
                            err
                        );
                        return;
                    }
                };

                let mut purged_count = 0;
                while let Ok(Some(key)) = keys.try_next().await {
                    match self.store.purge(&key).await {
                        Ok(_) => purged_count += 1,
                        Err(e) => {
                            log::error!("Failed to purge stored flow key={} error={}", key, e)
                        }
                    }
                }

                let mut stored_count = 0;
                for flow in flows.flows {
                    let key = get_flow_identifier(&flow);
                    let bytes = flow.encode_to_vec();
                    match self.store.put(key.clone(), bytes.into()).await {
                        Ok(_) => {
                            stored_count += 1;
                            log::debug!("Stored replacement flow key={}", key);
                        }
                        Err(err) => log::error!(
                            "Failed to store replacement flow key={} error={:?}",
                            key,
                            err
                        ),
                    };
                }
                log::info!(
                    "Finished replacing stored flows received_count={} purged_count={} stored_count={}",
                    received_count,
                    purged_count,
                    stored_count
                );
            }
            Data::ModuleConfigurations(action_configurations) => {
                let (project_count, config_count) = module_config_stats(&action_configurations);
                log::debug!(
                    "Received module configurations from flow stream module_identifier={} project_count={} config_count={}",
                    action_configurations.module_identifier,
                    project_count,
                    config_count
                );

                match self.action_config_tx.send(action_configurations) {
                    Ok(receiver_count) => log::debug!(
                        "Broadcasted module configurations to action forwarders receiver_count={}",
                        receiver_count
                    ),
                    Err(err) => {
                        log::warn!("No action configuration receivers available: {:?}", err);
                    }
                }
            }
        }
    }

    pub async fn init_flow_stream(&mut self) -> Result<(), tonic::Status> {
        self.sagittarius_ready.store(false, Ordering::SeqCst);

        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            FlowLogonRequest {},
        );

        let response = match self.client.update(request).await {
            Ok(res) => {
                log::info!("Successfully established a Stream (for Flows)");
                self.sagittarius_ready.store(true, Ordering::SeqCst);
                res
            }
            Err(status) => {
                self.sagittarius_ready.store(false, Ordering::SeqCst);
                log::warn!("Failed to establish Flow stream: {:?}", status);
                return Err(status);
            }
        };

        let mut stream = response.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(res) => {
                    self.handle_response(res).await;
                }
                Err(status) => {
                    self.sagittarius_ready.store(false, Ordering::SeqCst);
                    log::warn!("Flow stream error (will reconnect): {:?}", status);
                    return Err(status);
                }
            };
        }

        // Stream ended without an explicit error
        self.sagittarius_ready.store(false, Ordering::SeqCst);
        log::warn!("Flow stream ended (server closed). Will reconnect.");
        Err(tonic::Status::unavailable("flow stream ended"))
    }
}
