use std::sync::Arc;

use code0_flow::flow_store::{
    connection::create_flow_store_connection,
    service::{FlowStoreService, FlowStoreServiceBase},
};
use configuration::config::Config;
use sagittarius::flow_service_client_impl::SagittariusFlowClient;
use tokio::sync::Mutex;

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
    }
}
