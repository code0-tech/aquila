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
    // Configure logging
    std::env::set_var("RUST_LOG", "info");
    json_env_logger2::init();
    json_env_logger2::panic_hook();

    // Config creation
    let config = Config::new();

    // Redis connection
    let client = build_connection(config.redis_url.clone());
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));

    // Startup
    let mut startup = StartConfigurationBase::new(connection, config).await;
    startup.init_flows_from_sagittarius().await;
    startup.init_flows_from_json().await
}