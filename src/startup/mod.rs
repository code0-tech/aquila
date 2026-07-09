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
        config.nats.url,
        config.nats.bucket
    );

    // Create connection to JetStream
    let client = match async_nats::connect(config.nats.url.clone()).await {
        Ok(client) => {
            log::info!(
                "Aquila messaging dependency is ready dependency=nats url={}",
                config.nats.url
            );
            client
        }
        Err(err) => {
            panic!("Failed to connect to NATS server: {:?}", err)
        }
    };

    let jet_stream = async_nats::jetstream::new(client.clone());

    match jet_stream
        .create_key_value(Config {
            bucket: config.nats.bucket.clone(),
            ..Default::default()
        })
        .await
    {
        Ok(_) => log::debug!(
            "NATS key-value bucket is available bucket={}",
            config.nats.bucket
        ),
        Err(err) => log::debug!(
            "NATS key-value bucket creation skipped or failed; attempting to open existing bucket bucket={} error={:?}",
            config.nats.bucket,
            err
        ),
    }

    let kv_store = match jet_stream.get_key_value(config.nats.bucket.clone()).await {
        Ok(kv) => {
            log::info!(
                "Aquila flow store is ready backend=nats_jetstream bucket={}",
                config.nats.bucket
            );
            Arc::new(kv)
        }
        Err(err) => {
            panic!(
                "Failed to open NATS key-value store bucket={} error={:?}",
                config.nats.bucket, err
            )
        }
    };

    if config.is_static() {
        log::info!("Selected Aquila startup mode mode=static source=local_flow_export");
        static_mode::run(config, app_readiness, service_config, client, kv_store).await;
        return;
    }

    log::info!("Selected Aquila startup mode mode=dynamic source=sagittarius");
    dynamic_mode::run(config, app_readiness, service_config, client, kv_store).await;
}
