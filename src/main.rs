extern crate core;

use tokio::sync::Mutex;
use std::sync::{Arc};
use ::redis::aio::MultiplexedConnection;
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
    let client = build_connection();
    let con = client.get_multiplexed_async_connection().await.unwrap();
    let connection = Arc::new(Mutex::new(Box::new(con)));

    init_endpoints(connection).await;
    init_client(connection).await;
}

async fn init_endpoints(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) {
    let has_grpc_enabled_env = match std::env::var("ENABLE_GRPC_UPDATE") {
        Ok(env) => env,
        Err(var_error) => {
            print!("Env. Variable ENABLE_GRPC_UPDATE wasn't found. Reason: {var_error}");
            return;
        }
    };

    let has_grpc_enabled: bool = match has_grpc_enabled_env.parse() {
        Ok(env) => env,
        Err(parse_error) => {
            print!("Can't parse variable. Reason: {parse_error}");
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
        Ok(_) => print!("Started Flow-Endpoint"),
        Err(server_error) => panic!("Can't start Flow-Endpoint {server_error}")
    }
}

async fn init_client(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) {

    let client = match FlowSagittariusServiceClient::connect("https://[::1]:50051").await {
        Ok(res) => res,
        Err(start_error) => {
            print!("Can't start client {start_error}");
            return;
        }
    };

    let mut flow_client = FlowClient::new(connection_arc, client).await;

    let has_scheduled_enabled_env = match std::env::var("ENABLE_SCHEDULED_UPDATE") {
        Ok(env) => env,
        Err(var_error) => {
            print!("Env. Variable ENABLE_SCHEDULED_UPDATE wasn't found. Reason: {var_error}");
            return;
        }
    };

    let has_scheduled_enabled: bool = match has_scheduled_enabled_env.parse() {
        Ok(env) => env,
        Err(parse_error) => {
            print!("Can't parse variable. Reason: {parse_error}");
            return;
        }
    };

    if !has_scheduled_enabled {
        flow_client.send_get_flow_request().await;
        return;
    }

    todo!("Start Timer ==> by scheduled env")
}