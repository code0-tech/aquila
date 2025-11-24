use crate::sagittarius::test_execution_client_impl::SagittariusTestExecutionServiceClient;
use crate::{configuration::Config as AquilaConfig, flow::get_flow_identifier};
use async_nats::jetstream::kv::Config;
use code0_flow::flow_config::load_env_file;
use prost::Message;
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use serde_json::from_str;
use server::AquilaGRPCServer;
use std::{fs::File, io::Read, sync::Arc};
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

    //Create connection to JetStream
    let client = match async_nats::connect(config.nats_url.clone()).await {
        Ok(client) => {
            log::info!("Connected to NATS");
            client
        }
        Err(err) => {
            log::error!("Failed to connect to NATS: {:?}", err);
            panic!("Failed to connect to NATS server: {:?}", err)
        },
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
        },
    };

    if config.is_static() {
        log::info!("Starting with static configuration");
        init_flows_from_json(config.flow_fallback_path, kv_store).await;
        return;
    }

    let server = AquilaGRPCServer::new(&config);
    let backend_url_flow = config.backend_url.clone();
    let runtime_token_flow = config.runtime_token.clone();
    let kv_for_flow = kv_store.clone();

    let mut server_task = tokio::spawn(async move {
        if let Err(err) = server.start().await {
            log::error!("gRPC server error: {:?}", err);
        } else {
            log::info!("gRPC server stopped gracefully");
        }
    });

    let mut flow_task = tokio::spawn(async move {
        let flow_client =
            SagittariusFlowClient::new(backend_url_flow, kv_for_flow, runtime_token_flow).await;
        let mut flow_client = flow_client;

        flow_client.init_flow_stream().await;
        log::warn!("Flow stream task exited");
    });

    tokio::select! {
        _ = &mut server_task => {
            log::warn!("gRPC server task finished, shutting down");
        }
        _ = &mut flow_task => {
            log::warn!("Flow stream task finished, shutting down");
        }
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C/Exit signal received, shutting down");
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
}
