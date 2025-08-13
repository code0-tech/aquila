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
        Ok(client) => client,
        Err(err) => panic!("Failed to connect to NATS server: {}", err),
    };

    let jetstream = async_nats::jetstream::new(client.clone());

    let _ = jetstream
        .create_key_value(Config {
            bucket: config.nats_bucket.clone(),
            ..Default::default()
        })
        .await;

    let kv_store = match jetstream.get_key_value(config.nats_bucket.clone()).await {
        Ok(kv) => Arc::new(kv),
        Err(err) => panic!("Failed to get key-value store: {}", err),
    };

    //Create connection to Sagittarius if the type is hybrid
    if !config.is_static() {
        let server = AquilaGRPCServer::new(&config);

        match server.start().await {
            Ok(_) => {
                log::info!("Server started successfully");
            }
            Err(err) => {
                log::error!("Failed to start server: {:?}", err);
                panic!("Failed to start server");
            }
        };

        let mut sagittarius_client =
            SagittariusFlowClient::new(config.backend_url, kv_store, config.runtime_token).await;

        sagittarius_client.init_flow_stream().await;
    } else {
        init_flows_from_json(config.flow_fallback_path, kv_store).await
    }
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
        match flow_store_client.put(key, bytes.into()).await {
            Ok(_) => log::info!("Flow updated successfully"),
            Err(err) => log::error!("Failed to update flow. Reason: {:?}", err),
        };
    }
}
