extern crate core;

use std::str::FromStr;
use tokio::sync::Mutex;
use std::sync::{Arc};
use ::redis::aio::MultiplexedConnection;
use clokwerk::{AsyncScheduler};
use log::{error, info};
use tonic::transport::Server;
use crate::client::flow_client::FlowClient;
use crate::endpoint::configuration_endpoint::flow_aquila_service_server::FlowAquilaServiceServer;
use crate::endpoint::configuration_endpoint::flow_sagittarius_service_client::FlowSagittariusServiceClient;
use crate::endpoint::flow_endpoint::FlowEndpoint;
use crate::redis::build_connection;

mod client;
mod endpoint;
mod redis;

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

async fn init_endpoints(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) {
    let has_grpc_enabled_env = match std::env::var("ENABLE_GRPC_UPDATE") {
        Ok(env) => env,
        Err(var_error) => {
            error!("Env. Variable ENABLE_GRPC_UPDATE wasn't found. Reason: {var_error}");
            return;
        }
    };

    let has_grpc_enabled: bool = match has_grpc_enabled_env.parse() {
        Ok(env) => env,
        Err(parse_error) => {
            error!("Can't parse variable. Reason: {parse_error}");
            return;
        }
    };

    if !has_grpc_enabled {
        return;
    }

    let addr = "[::1]:50051".parse().unwrap();
    let service = FlowEndpoint::new(connection_arc);

    let server = Server::builder()
        .add_service(FlowAquilaServiceServer::new(service))
        .serve(addr).await;

    match server {
        Ok(_) => info!("Started Flow-Endpoint"),
        Err(server_error) => error!("Can't start Flow-Endpoint {server_error}")
    }
}

async fn init_client(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) {

    let client = match FlowSagittariusServiceClient::connect("https://[::1]:50051").await {
        Ok(res) => res,
        Err(start_error) => {
            error!("Can't start client {start_error}");
            return;
        }
    };

    let mut flow_client = FlowClient::new(connection_arc, client).await;

    let has_scheduled_enabled_env = match std::env::var("ENABLE_SCHEDULED_UPDATE") {
        Ok(env) => env,
        Err(var_error) => {
            error!("Env. Variable ENABLE_SCHEDULED_UPDATE wasn't found. Reason: {var_error}");
            return;
        }
    };

    let has_scheduled_enabled: bool = match has_scheduled_enabled_env.parse() {
        Ok(env) => env,
        Err(parse_error) => {
            error!("Can't parse variable. Reason: {parse_error}");
            return;
        }
    };

    if !has_scheduled_enabled {
        flow_client.send_get_flow_request().await;
        return;
    }

    let schedule_interval_env = match std::env::var("UPDATE_SCHEDULE_INTERVAL") {
        Ok(interval_env) => interval_env,
        Err(err) => {
            error!("ENABLE_SCHEDULED_UPDATE true but UPDATE_SCHEDULE_INTERVAL not set: {err}");
            return;
        }
    };

    let schedule_interval = match u32::from_str(&schedule_interval_env) {
        Ok(interval) => interval,
        Err(err) => {
            error!("Cannot parse UPDATE_SCHEDULE_INTERVAL to u32: {err}");
            return;
        }
    };

    let mut scheduler = AsyncScheduler::new();

    todo!("Work on the shit below");
    /*
    scheduler.every(schedule_interval.seconds()).run(move || {
        async {
            let flw = flow_client_arc.lock().await;
            //flow_client.send_get_flow_request().await;
        }
    });
    */
}