use crate::configuration::start_configuration::StartConfiguration;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::configuration::config::Config;
use crate::data::redis::build_connection;
use crate::endpoint::action_endpoint::ActionEndpoint;

mod client;
mod endpoint;
mod configuration;
mod data;

#[tokio::main]
async fn main() {
    json_env_logger2::init();
    json_env_logger2::panic_hook();

    let config = Config::new();
    let client = build_connection();
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));

    StartConfiguration::init(connection, config).await;
    ActionEndpoint::new().await;
}