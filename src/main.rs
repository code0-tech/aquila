use crate::{
    configuration::{config::Config as AquilaConfig, state::AppReadiness},
    flow::get_flow_identifier,
    sagittarius::retry::create_channel_with_retry,
};
use async_nats::jetstream::kv::Config;
use code0_flow::flow_config::load_env_file;
use prost::Message;
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use serde_json::from_str;
use server::AquilaGRPCServer;
use std::{fs::File, io::Read, sync::Arc, time::Duration};
use tucana::shared::Flows;

pub mod authorization;
pub mod configuration;
pub mod flow;
pub mod sagittarius;
pub mod server;
pub mod stream;

#[tokio::main]
async fn main() {
    log::info!("Starting Aquila...");

    // Configure Logging
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Load environment variables from .env file
    load_env_file();
    let config = AquilaConfig::new();
    let app_readiness = AppReadiness::new();

    //Create connection to JetStream
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

    if config.is_static() {
        log::info!("Starting with static configuration");
        init_flows_from_json(config.flow_fallback_path, kv_store).await;
        return;
    }

    let backend_url_flow = config.backend_url.clone();
    let sagittarius_channel = create_channel_with_retry(
        "Sagittarius Endpoint",
        backend_url_flow,
        app_readiness.sagittarius_ready.clone(),
    )
    .await;
    let server = AquilaGRPCServer::new(&config, app_readiness.clone(), sagittarius_channel.clone());
    let kv_for_flow = kv_store.clone();

    let mut server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            log::error!("gRPC server error: {:?}", err);
        } else {
            log::info!("gRPC server stopped gracefully");
        }
    });

    let env = match config.environment {
        code0_flow::flow_config::environment::Environment::Development => {
            String::from("DEVELOPMENT")
        }
        code0_flow::flow_config::environment::Environment::Staging => String::from("STAGING"),
        code0_flow::flow_config::environment::Environment::Production => String::from("PRODUCTION"),
    };

    let mut flow_task = tokio::spawn(async move {
        let mut backoff = Duration::from_millis(200);
        let max_backoff = Duration::from_secs(10);

        loop {
            let ch = create_channel_with_retry(
                "Sagittarius Stream",
                config.backend_url.clone(),
                app_readiness.sagittarius_ready.clone(),
            )
            .await;

            let mut flow_client = SagittariusFlowClient::new(
                kv_for_flow.clone(),
                env.clone(),
                config.runtime_token.clone(),
                ch,
                app_readiness.sagittarius_ready.clone(),
            );

            match flow_client.init_flow_stream().await {
                Ok(_) => {
                    log::warn!("Flow stream ended cleanly. Reconnecting...");
                }
                Err(e) => {
                    log::warn!("Flow stream dropped: {:?}. Reconnecting...", e);
                }
            }

            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
        }
    });

    #[cfg(unix)]
    let sigterm = async {
        use tokio::signal::unix::{SignalKind, signal};

        let mut term = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        term.recv().await;
    };

    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = &mut server_task => {
            log::warn!("gRPC server task finished, shutting down");
            flow_task.abort();
        }
        _ = &mut flow_task => {
            log::warn!("Flow stream task finished, shutting down");
            server_task.abort();
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
            server_task.abort();
            flow_task.abort();
        }
        _ = sigterm => {
            log::info!("SIGTERM received, shutting down");
            server_task.abort();
            flow_task.abort();
        }
    }

    log::info!("Aquila shutdown complete");
}

async fn init_flows_from_json(
    path: String,
    flow_store_client: Arc<async_nats::jetstream::kv::Store>,
) {
    let mut data = String::new();

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            panic!("There was a problem opening the file: {:?}", error);
        }
    };

    match file.read_to_string(&mut data) {
        Ok(_) => {
            print!("Successfully read data from file");
        }
        Err(error) => {
            panic!("There was a problem reading the file: {:?}", error);
        }
    }

    let flows: Flows = match from_str(&data) {
        Ok(flows) => flows,
        Err(error) => {
            panic!(
                "There was a problem deserializing the json file: {:?}",
                error
            );
        }
    };

    for flow in flows.flows {
        let key = get_flow_identifier(&flow);
        let bytes = flow.encode_to_vec();
        log::info!("Inserting flow with key {}", &key);
        match flow_store_client.put(key, bytes.into()).await {
            Ok(_) => log::info!("Flow updated successfully"),
            Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
        };
    }

    log::info!("Shutting down after successfully inserting all flows");
}
