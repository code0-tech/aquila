use core::task;
use std::fs::File;
use std::io::Read;
use std::str::FromStr;
use std::sync::Arc;
use clokwerk::{AsyncScheduler, TimeUnits};
use log::{error, info};
use redis::aio::MultiplexedConnection;
use tokio::sync::Mutex;
use tonic::transport::{Channel, Server};
use tucana_internal::internal::Flow;
use tucana_internal::internal::flow_aquila_service_server::FlowAquilaServiceServer;
use tucana_internal::internal::flow_sagittarius_service_client::FlowSagittariusServiceClient;
use crate::client::flow_client::FlowClient;
use crate::endpoint::flow_endpoint::FlowEndpoint;

pub struct StartConfiguration {
    connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>,
    flow_client: FlowClient,
}

impl StartConfiguration {
    pub async fn new(connection_arc: Arc<Mutex<Box<MultiplexedConnection>>>) -> Self {
        let client = match FlowSagittariusServiceClient::connect("https://[::1]:50051").await {
            Ok(res) => res,
            Err(start_error) => {
                panic!("Can't start client {start_error}");
            }
        };

        let flow_client = FlowClient::new(connection_arc.clone(), client).await;
        Self { connection_arc, flow_client }
    }


    pub async fn init_endpoints(&self) {
        let has_grpc_enabled = get_env_with_default("ENABLE_GRPC_UPDATE", false);

        if !has_grpc_enabled {
            return;
        }

        let addr = "[::1]:50051".parse().unwrap();
        let service = FlowEndpoint::new(self.connection_arc.clone());

        let server = Server::builder()
            .add_service(FlowAquilaServiceServer::new(service))
            .serve(addr).await;

        match server {
            Ok(_) => info!("Started Flow-Endpoint"),
            Err(server_error) => error!("Can't start Flow-Endpoint {server_error}")
        }
    }

    pub async fn init_client(&mut self) {
        let has_scheduled_enabled = get_env_with_default("ENABLE_SCHEDULED_UPDATE", false);

        if !has_scheduled_enabled {
            self.flow_client.send_get_flow_request().await;
            return;
        }

        let schedule_interval = get_env_with_default("UPDATE_SCHEDULE_INTERVAL", 0);
        let mut scheduler = AsyncScheduler::new();

        let flow_client_arc = Arc::new(Mutex::new(self.flow_client.clone()));

        scheduler
            .every(schedule_interval.seconds())
            .run(move ||  { 
                let local_flow_client = flow_client_arc.clone();
                
                async move {
                    let mut current_flow_client = local_flow_client.lock().await;
                    current_flow_client.send_get_flow_request().await
                }
            });
            
    }

    pub async fn init_json(mut self) {
        let has_grpc = get_env_with_default("ENABLE_GRPC_UPDATE", false);
        let has_endpoint = get_env_with_default("ENABLE_SCHEDULED_UPDATE", false);

        if has_grpc && has_endpoint {
            return;
        }

        let mut data = String::new();
        let mut file = File::open("./configuration/configuration.json").expect("Cannot open file");
        
        file.read_to_string(&mut data).expect("Cannot read file");
        let flows: Vec<Flow> = serde_json::from_str(&data).expect("Failed to parse JSON to list of flows");

        self.flow_client.insert_flows(flows).await;
    }
}

fn get_env_with_default<T>(name: &str, default: T) -> T
where
    T: FromStr,
{
    let env_variable = match std::env::var(name) {
        Ok(env) => env,
        Err(find_error) => {
            error!("Env. Variable {name} wasn't found. Reason: {find_error}");
            return default;
        }
    };

    env_variable.parse::<T>().unwrap_or_else(|_| default)
}