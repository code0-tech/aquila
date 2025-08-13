use crate::{
    configuration::Config,
    sagittarius::{
        data_type_service_client_impl::SagittariusDataTypeServiceClient,
        flow_type_service_client_impl::SagittariusFlowTypeServiceClient,
        runtime_function_service_client_impl::SagittariusRuntimeFunctionServiceClient,
    },
};
use data_type_service_server_impl::AquilaDataTypeServiceServer;
use flow_type_service_server_impl::AquilaFlowTypeServiceServer;
use log::info;
use runtime_function_service_server_impl::AquilaRuntimeFunctionServiceServer;
use std::net::SocketAddr;
use tonic::transport::Server;
use tucana::aquila::{
    data_type_service_server::DataTypeServiceServer,
    flow_type_service_server::FlowTypeServiceServer,
    runtime_function_definition_service_server::RuntimeFunctionDefinitionServiceServer,
};

mod data_type_service_server_impl;
mod flow_type_service_server_impl;
mod runtime_function_service_server_impl;

pub struct AquilaGRPCServer {
    token: String,
    sagittarius_url: String,
    nats_url: String,
    address: SocketAddr,
}

impl AquilaGRPCServer {
    pub fn new(config: &Config) -> Self {
        let address = match format!("127.0.0.1:{}", config.grpc_port).parse() {
            Ok(addr) => {
                info!("Listening on {:?}", &addr);
                addr
            }
            Err(e) => panic!("Failed to parse address: {:?}", e),
        };

        AquilaGRPCServer {
            token: config.runtime_token.clone(),
            sagittarius_url: config.backend_url.clone(),
            nats_url: config.nats_url.clone(),
            address,
        }
    }

    pub async fn start(&self) -> std::result::Result<(), tonic::transport::Error> {
        let data_type_service = SagittariusDataTypeServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        log::info!("DataTypeService started");

        let flow_type_service = SagittariusFlowTypeServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        log::info!("FlowTypeService started");

        let runtime_function_service = SagittariusRuntimeFunctionServiceClient::new_arc(
            self.sagittarius_url.clone(),
            self.token.clone(),
        )
        .await;

        log::info!("RuntimeFunctionService started");

        let health_service = code0_flow::flow_health::HealthService::new(self.nats_url.clone());
        log::info!("HealthService started");

        let data_type_server = AquilaDataTypeServiceServer::new(data_type_service.clone());
        let flow_type_server = AquilaFlowTypeServiceServer::new(flow_type_service.clone());
        let runtime_function_server =
            AquilaRuntimeFunctionServiceServer::new(runtime_function_service.clone());

        log::info!("Starting gRPC Server...");

        Server::builder()
            .add_service(tonic_health::pb::health_server::HealthServer::new(
                health_service,
            ))
            .add_service(DataTypeServiceServer::new(data_type_server))
            .add_service(FlowTypeServiceServer::new(flow_type_server))
            .add_service(RuntimeFunctionDefinitionServiceServer::new(
                runtime_function_server,
            ))
            .serve(self.address.clone())
            .await
    }
}
