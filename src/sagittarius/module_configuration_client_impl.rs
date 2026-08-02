//! Client for Sagittarius' module configuration stream: forwards module
//! configuration updates onto `action_config_tx` for the action forwarders to
//! pick up. This used to arrive embedded in the flow synchronization stream
//! (see [`super::flow_service_client_impl`]) but now has its own dedicated
//! stream.

use futures::StreamExt;
use tokio::sync::broadcast;
use tonic::{Extensions, Request, transport::Channel};
use tucana::sagittarius_gateway::{
    ModuleConfigurationRequest, module_service_client::ModuleServiceClient,
};

use crate::authorization::authorization::get_authentication_metadata;

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
pub struct SagittariusModuleConfigurationClient {
    client: ModuleServiceClient<Channel>,
    token: String,
    action_config_tx: broadcast::Sender<tucana::shared::ModuleConfigurations>,
}

impl SagittariusModuleConfigurationClient {
    pub fn new(
        channel: Channel,
        token: String,
        action_config_tx: broadcast::Sender<tucana::shared::ModuleConfigurations>,
    ) -> Self {
        Self {
            client: ModuleServiceClient::new(channel),
            token,
            action_config_tx,
        }
    }

    fn handle_response(&self, module_configurations: tucana::shared::ModuleConfigurations) {
        let (project_count, config_count) = module_config_stats(&module_configurations);
        log::debug!(
            "Received module configurations module_identifier={} project_count={} config_count={}",
            module_configurations.module_identifier,
            project_count,
            config_count
        );

        match self.action_config_tx.send(module_configurations) {
            Ok(receiver_count) => log::debug!(
                "Broadcasted module configurations to action forwarders receiver_count={}",
                receiver_count
            ),
            Err(err) => {
                log::warn!("No action configuration receivers available: {:?}", err);
            }
        }
    }

    /// Opens the module configuration stream and services it until it ends
    /// or errors, at which point the caller is expected to reconnect.
    pub async fn init_configuration_stream(&mut self) -> Result<(), tonic::Status> {
        let request = Request::from_parts(
            get_authentication_metadata(&self.token),
            Extensions::new(),
            ModuleConfigurationRequest {},
        );

        let response = match self.client.configurations(request).await {
            Ok(res) => {
                log::info!("Sagittarius module configuration stream established");
                res
            }
            Err(status) => {
                log::warn!(
                    "Sagittarius module configuration stream connection failed status={:?}",
                    status
                );
                return Err(status);
            }
        };

        let mut stream = response.into_inner();

        while let Some(result) = stream.next().await {
            match result {
                Ok(res) => {
                    if let Some(module_configurations) = res.module_configurations {
                        self.handle_response(module_configurations);
                    } else {
                        log::warn!("Received empty Sagittarius module configuration response");
                    }
                }
                Err(status) => {
                    log::warn!(
                        "Sagittarius module configuration stream failed; reconnecting status={:?}",
                        status
                    );
                    return Err(status);
                }
            }
        }

        log::warn!("Sagittarius closed the module configuration stream; reconnecting");
        Err(tonic::Status::unavailable(
            "module configuration stream ended",
        ))
    }
}
