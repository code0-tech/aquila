use crate::configuration::config::Config;
use crate::configuration::start_configuration::{StartConfiguration, StartConfigurationBase};
use aquila_cache::build_connection;
use std::env::set_var;
use std::sync::Arc;
use tokio::sync::Mutex;

mod client;
mod configuration;
mod server;
mod service;

#[tokio::main]
async fn main() {
    // Configure logging
    set_var("RUST_LOG", "info");
    json_env_logger2::init();
    json_env_logger2::panic_hook();

    // Config creation
    let config = Config::new();

    // Redis connection
    let client = build_connection(config.redis_url.clone());

    let con = match client.get_multiplexed_async_connection().await {
        Ok(con) => con,
        Err(err) => {
            panic!("Failed to connect to server: {}", err);
        }
    };

    let connection = Arc::new(Mutex::new(Box::new(con)));

    // Startup
    let mut startup = StartConfigurationBase::new(connection, config).await;
    startup.init_flows_from_sagittarius().await;
    startup.init_flows_from_json().await
}
