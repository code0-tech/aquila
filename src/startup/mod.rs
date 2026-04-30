pub mod dynamic_mode;
pub mod static_mode;

use crate::configuration::{
    config::Config as AquilaConfig, service::ServiceConfiguration, state::AppReadiness,
};
use async_nats::jetstream::kv::Config;
use std::sync::Arc;

pub async fn run(
    config: AquilaConfig,
    app_readiness: AppReadiness,
    service_config: ServiceConfiguration,
) {
    log::info!(
        "Bootstrapping startup mode={} nats_url={} nats_bucket={}",
        if config.is_static() {
            "static"
        } else {
            "dynamic"
        },
        config.nats_url,
        config.nats_bucket
    );

    // Create connection to JetStream
    let client = match async_nats::connect(config.nats_url.clone()).await {
        Ok(client) => {
            log::info!("Connected to NATS");
            client
        }
        Err(err) => {
            log::error!("Failed to connect to NATS: {:?}", err);
            panic!("Failed to connect to NATS server: {:?}", err)
        }
    };

    let jet_stream = async_nats::jetstream::new(client.clone());

    let _ = jet_stream
        .create_key_value(Config {
            bucket: config.nats_bucket.clone(),
            ..Default::default()
        })
        .await;
    log::debug!("Ensured NATS key-value bucket exists");

    let kv_store = match jet_stream.get_key_value(config.nats_bucket.clone()).await {
        Ok(kv) => {
            log::info!("Connected to JetStream");
            Arc::new(kv)
        }
        Err(err) => {
            log::error!("Failed to get key-value store: {:?}", err);
            panic!("Failed to get key-value store: {:?}", err)
        }
    };

    if config.is_static() {
        log::info!("Starting with static configuration");
        static_mode::run(config, app_readiness, service_config, client, kv_store).await;
        return;
    }

    log::info!("Starting with dynamic configuration");
    dynamic_mode::run(config, app_readiness, service_config, client, kv_store).await;
}
