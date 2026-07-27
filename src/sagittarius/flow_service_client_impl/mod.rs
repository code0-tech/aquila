//! Client for Sagittarius' flow synchronization stream: keeps Aquila's local
//! flow KV store in sync with whatever Sagittarius considers the current
//! set, and rebroadcasts action module configuration updates it receives
//! along the way.
//!
//! - [`flow_store`] applies the sync operations (delete/replace/update) to the KV store.
//! - [`dev_export`] mirrors the synced flows to a local JSON file, development only,
//!   updating it incrementally for single-flow updates/deletes and wholesale on replace.

mod dev_export;
mod flow_store;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use tokio::sync::broadcast;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius::{
    FlowLogonRequest, FlowResponse, flow_response::Data, flow_service_client::FlowServiceClient,
};

use crate::{authorization::authorization::get_authorization_metadata, telemetry::metrics};

fn module_config_stats(configs: &tucana::shared::ModuleConfigurations) -> (usize, usize) {
    let project_count = configs.module_configurations.len();
    let config_count = configs
        .module_configurations
        .iter()
        .map(|project_cfg| project_cfg.module_configurations.len())
        .sum();

    (project_count, config_count)
}

#[derive(Clone)]
pub struct SagittariusFlowClient {
    store: Arc<async_nats::jetstream::kv::Store>,
    client: FlowServiceClient<Channel>,
    env: String,
    token: String,
    /// Path to mirror synced flows to when running in development, same
    /// path static mode loads its fallback export from.
    flow_export_path: String,
    /// Flipped to `true` once the sync stream is established, so other
    /// components can hold off on work that depends on Sagittarius state
    /// actually being loaded.
    sagittarius_ready: Arc<AtomicBool>,
    action_config_tx: broadcast::Sender<tucana::shared::ModuleConfigurations>,
}

impl SagittariusFlowClient {
    pub fn new(
        store: Arc<async_nats::jetstream::kv::Store>,
        env: String,
        token: String,
        flow_export_path: String,
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
            flow_export_path,
            sagittarius_ready,
            action_config_tx,
        }
    }

    fn is_development(&self) -> bool {
        self.env == "DEVELOPMENT"
    }

    /// Applies one message from the flow sync stream to local state.
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

                if self.is_development() {
                    dev_export::remove_flow(&self.flow_export_path, id).await;
                }

                let deleted_count = flow_store::delete_flow(&self.store, id).await;

                if deleted_count == 0 {
                    metrics::flow_operation("delete", "not_found", 1);
                    log::warn!("Flow deletion matched no stored keys flow_id={}", id);
                } else {
                    metrics::flow_operation("delete", "success", 1);
                    log::info!(
                        "Flow deleted successfully id={} deleted_keys={}",
                        id,
                        deleted_count
                    );
                }
            }
            Data::UpdatedFlow(flow) => {
                let flow_id = flow.flow_id;

                if self.is_development() {
                    dev_export::upsert_flow(&self.flow_export_path, flow.clone()).await;
                }

                let (key, result) = flow_store::store_flow(&self.store, &flow).await;
                match result {
                    Ok(()) => {
                        metrics::flow_operation("update", "success", 1);
                        log::info!("Stored flow update flow_id={} key={}", flow_id, key)
                    }
                    Err(err) => {
                        metrics::flow_operation("update", "failure", 1);
                        log::error!(
                            "Failed to store flow update flow_id={} key={} error={:?}",
                            flow_id,
                            key,
                            err
                        )
                    }
                };
            }
            Data::Flows(flows) => {
                let received_count = flows.flows.len();
                log::info!(
                    "Replacing stored flows from Sagittarius received_count={}",
                    received_count
                );

                if self.is_development() {
                    dev_export::overwrite(&self.flow_export_path, flows.clone()).await;
                }

                let (purged_count, stored_count) = flow_store::replace_all(&self.store, flows).await;
                log::info!(
                    "Finished replacing stored flows received_count={} purged_count={} stored_count={}",
                    received_count,
                    purged_count,
                    stored_count
                );
                metrics::flow_operation("replace", "success", stored_count as u64);
                metrics::flow_operation(
                    "replace",
                    "failure",
                    received_count.saturating_sub(stored_count) as u64,
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

    /// Opens the flow sync stream and services it until it ends or errors,
    /// at which point the caller is expected to reconnect.
    pub async fn init_flow_stream(&mut self) -> Result<(), tonic::Status> {
        self.sagittarius_ready.store(false, Ordering::SeqCst);

        let request = Request::from_parts(
            get_authorization_metadata(&self.token),
            Extensions::new(),
            FlowLogonRequest {},
        );

        let response = match self.client.update(request).await {
            Ok(res) => {
                log::info!("Sagittarius flow synchronization stream established");
                self.sagittarius_ready.store(true, Ordering::SeqCst);
                res
            }
            Err(status) => {
                self.sagittarius_ready.store(false, Ordering::SeqCst);
                log::warn!(
                    "Sagittarius flow synchronization stream connection failed status={:?}",
                    status
                );
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
                    log::warn!(
                        "Sagittarius flow synchronization stream failed; reconnecting status={:?}",
                        status
                    );
                    return Err(status);
                }
            };
        }

        // Stream ended without an explicit error
        self.sagittarius_ready.store(false, Ordering::SeqCst);
        log::warn!("Sagittarius closed the flow synchronization stream; reconnecting");
        Err(tonic::Status::unavailable("flow stream ended"))
    }
}
