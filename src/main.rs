use crate::configuration::start_configuration::{StartConfiguration, StartConfigurationBase};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::configuration::config::Config;
use crate::data::redis::build_connection;

mod client;
mod configuration;
mod data;
mod service;
mod server;

#[tokio::main]
async fn main() {
    json_env_logger2::init();
    json_env_logger2::panic_hook();

    let config = Config::new();
    let client = build_connection(config.backend_url.clone());
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));
    
    let mut startup = StartConfigurationBase::new(connection, config).await;
    startup.init_flows_from_sagittarius().await;
    startup.init_flows_from_json().await
}