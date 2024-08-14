extern crate core;

use std::str::FromStr;
use tokio::sync::Mutex;
use std::sync::{Arc};
use ::redis::aio::MultiplexedConnection;
use clokwerk::{AsyncScheduler};
use log::{error, info};
use tonic::transport::Server;
use tucana_internal::internal::flow_aquila_service_server::FlowAquilaServiceServer;
use tucana_internal::internal::flow_sagittarius_service_client::FlowSagittariusServiceClient;
use crate::client::flow_client::FlowClient;
use crate::configuration::configuration::{init_client, init_endpoints};
use crate::endpoint::flow_endpoint::FlowEndpoint;
use crate::redis::build_connection;

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

    init_endpoints(connection.clone()).await;
    init_client(connection).await;
}
