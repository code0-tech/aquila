use code0_flow::flow_store::{
    connection::create_flow_store_connection,
    service::{FlowStoreService, FlowStoreServiceBase},
};
use configuration::config::Config;
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use serde_json::from_str;
use std::{fs::File, io::Read, sync::Arc};
use tokio::sync::Mutex;
use tucana::sagittarius::Flows;

pub mod authorization;
pub mod configuration;
pub mod sagittarius;
pub mod stream;

#[tokio::main]
async fn main() {
    let config = Config::new();
    config.print_config();

    let flow_store = create_flow_store_connection(config.redis_url.clone()).await;
    let flow_store_client = Arc::new(Mutex::new(FlowStoreService::new(flow_store).await));

    //Create connection to Sagittarius if the type is hybrid
    if !config.is_static() {
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
