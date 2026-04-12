pub mod dynamic_mode;
pub mod static_mode;

use crate::{
    configuration::{
        config::Config as AquilaConfig, service::ServiceConfiguration, state::AppReadiness,
    },
    sagittarius::retry::create_channel_with_retry,
    server::AquilaGRPCServer,
};
use async_nats::jetstream::kv::Config;
use std::sync::Arc;

pub async fn run(
    config: AquilaConfig,
    app_readiness: AppReadiness,
    action_config: ServiceConfiguration,
) {
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

    let backend_url_flow = config.backend_url.clone();
    let sagittarius_channel = create_channel_with_retry(
        "Sagittarius Endpoint",
        backend_url_flow,
        app_readiness.sagittarius_ready.clone(),
    )
    .await;

    let (action_config_tx, _) =
        tokio::sync::broadcast::channel::<tucana::shared::ActionConfigurations>(64);

    let server = AquilaGRPCServer::new(
        &config,
        app_readiness.clone(),
        sagittarius_channel.clone(),
        action_config,
        client.clone(),
        kv_store.clone(),
        action_config_tx.clone(),
    );

    let server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            log::error!("gRPC server error: {:?}", err);
        } else {
            log::info!("gRPC server stopped gracefully");
        }
    });

    if config.is_static() {
        log::info!("Starting with static configuration");
        static_mode::run(config.flow_fallback_path, kv_store, server_task).await;
        return;
    }

    dynamic_mode::run(
        config,
        app_readiness,
        kv_store,
        action_config_tx,
        server_task,
    )
    .await;
}
