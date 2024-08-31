use crate::configuration::start_configuration::StartConfiguration;
use crate::redis::build_connection;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::endpoint::action_endpoint::ActionEndpoint;

mod client;
mod endpoint;
mod redis;
mod configuration;
mod rabbitmq;
mod env;

#[tokio::main]
async fn main() {
    let result = dotenv::from_filename(".env");
    match result {
        Ok(_) => println!(".env file loaded successfully"),
        Err(e) => eprintln!("Error loading .env file: {}", e),
    }

    json_env_logger2::init();
    json_env_logger2::panic_hook();

    let client = build_connection();
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));

    let mut configuration = StartConfiguration::new(connection).await;
    configuration.init_endpoints().await;
    configuration.init_client().await;
    configuration.init_json().await;
    
    ActionEndpoint::new().await;
}