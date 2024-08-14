extern crate core;

use crate::configuration::start_configuration::{init_client, init_endpoints, init_json, StartConfiguration};
use crate::redis::build_connection;
use std::sync::Arc;
use tokio::sync::Mutex;

mod client;
mod endpoint;
mod redis;
mod configuration;

#[tokio::main]
async fn main() {
    json_env_logger2::init();
    json_env_logger2::panic_hook();

    let client = build_connection();
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));
    
    let configuration = StartConfiguration::
    init_endpoints(connection.clone()).await;
    init_client(connection.clone()).await;
    init_json(connection).await;
}
