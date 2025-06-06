use code0_flow::{
    flow_config::load_env_file,
    flow_store::{
        connection::create_flow_store_connection,
        service::{FlowStoreService, FlowStoreServiceBase},
    },
};
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use serde_json::from_str;
use server::AquilaGRPCServer;
use std::{fs::File, io::Read, sync::Arc};
use tokio::sync::Mutex;
use tucana::shared::Flows;

use crate::configuration::Config;

pub mod authorization;
pub mod configuration;
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
    let config = Config::new();

    let flow_store = create_flow_store_connection(config.redis_url.clone()).await;
    let flow_store_client = Arc::new(Mutex::new(FlowStoreService::new(flow_store).await));

    //Create connection to Sagittarius if the type is hybrid
    if !config.is_static() {
        let server = AquilaGRPCServer::new(
            config.runtime_token.clone(),
            config.backend_url.clone(),
            8080,
        );

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
            SagittariusFlowClient::new(config.backend_url, flow_store_client, config.runtime_token)
                .await;

        sagittarius_client.init_flow_stream().await;
    } else {
        init_flows_from_json(config.flow_fallback_path, flow_store_client).await
    }
}

async fn init_flows_from_json(path: String, flow_store_client: Arc<Mutex<FlowStoreService>>) {
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

    let mut store = flow_store_client.lock().await;
    let _ = store.insert_flows(flows).await;
}
