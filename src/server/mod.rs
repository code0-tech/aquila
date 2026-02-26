use crate::{
    configuration::{config::Config, state::AppReadiness},
    sagittarius::{
        data_type_service_client_impl::SagittariusDataTypeServiceClient,
        flow_type_service_client_impl::SagittariusFlowTypeServiceClient,
        runtime_function_service_client_impl::SagittariusRuntimeFunctionServiceClient,
        runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient,
    },
    server::runtime_status_service_server_impl::AquilaRuntimeStatusServiceServer,
};
use data_type_service_server_impl::AquilaDataTypeServiceServer;
use flow_type_service_server_impl::AquilaFlowTypeServiceServer;
use log::info;
use runtime_function_service_server_impl::AquilaRuntimeFunctionServiceServer;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tonic::{
    Request, Status,
    transport::{Channel, Server},
};
use tucana::aquila::{
    data_type_service_server::DataTypeServiceServer,
    flow_type_service_server::FlowTypeServiceServer,
    runtime_function_definition_service_server::RuntimeFunctionDefinitionServiceServer,
    runtime_status_service_server::RuntimeStatusServiceServer,
};

mod data_type_service_server_impl;
mod flow_type_service_server_impl;
mod runtime_function_service_server_impl;
mod runtime_status_service_server_impl;

pub struct AquilaGRPCServer {
    token: String,
    nats_url: String,
    address: SocketAddr,
    with_health_service: bool,
    app_readiness: AppReadiness,
    channel: Channel,
}

impl AquilaGRPCServer {
    pub fn new(config: &Config, app_readiness: AppReadiness, channel: Channel) -> Self {
        let address = match format!("{}:{}", config.grpc_host, config.grpc_port).parse() {
            Ok(addr) => {
                info!("Listening on {:?}", &addr);
                addr
            }
            Err(e) => panic!("Failed to parse address: {:?}", e),
        };

        AquilaGRPCServer {
            token: config.runtime_token.clone(),
            nats_url: config.nats_url.clone(),
            with_health_service: config.with_health_service,
            address,
            app_readiness,
            channel,
        }
    }

    pub async fn start(&self) -> Result<(), tonic::transport::Error> {
        let data_type_service = Arc::new(Mutex::new(SagittariusDataTypeServiceClient::new(
            self.channel.clone(),
            self.token.clone(),
        )));

        info!("DataTypeService started");

        let flow_type_service = Arc::new(Mutex::new(SagittariusFlowTypeServiceClient::new(
            self.channel.clone(),
            self.token.clone(),
        )));
        info!("FlowTypeService started");

        let runtime_function_service = Arc::new(Mutex::new(
            SagittariusRuntimeFunctionServiceClient::new(self.channel.clone(), self.token.clone()),
        ));

        info!("RuntimeFunctionService started");

        let runtime_status_service = Arc::new(Mutex::new(
            SagittariusRuntimeStatusServiceClient::new(self.channel.clone(), self.token.clone()),
        ));

        info!("RuntimeStatusService started");

        let data_type_server = AquilaDataTypeServiceServer::new(data_type_service.clone());
        let flow_type_server = AquilaFlowTypeServiceServer::new(flow_type_service.clone());
        let runtime_function_server =
            AquilaRuntimeFunctionServiceServer::new(runtime_function_service.clone());
        let runtime_status_server =
            AquilaRuntimeStatusServiceServer::new(runtime_status_service.clone());

        info!("Starting gRPC Server...");

        let readiness: Arc<AppReadiness> = Arc::new(self.app_readiness.clone());

        let intercept = {
            let readiness = readiness.clone();
            move |req: Request<()>| -> Result<Request<()>, Status> {
                if !readiness.is_ready() {
                    log::error!("Rejected a request because Sagittarius is not ready.");
                    Err(Status::unavailable(
                        "Service not ready, waiting on Sagittarius. Please retry again later!",
                    ))
                } else {
                    Ok(req)
                }
            }
        };

        if self.with_health_service {
            info!("Starting with HealthService");
            let health_service = code0_flow::flow_health::HealthService::new(self.nats_url.clone());

            Server::builder()
                .add_service(tonic_health::pb::health_server::HealthServer::new(
                    health_service,
                ))
                .add_service(DataTypeServiceServer::with_interceptor(
                    data_type_server,
                    intercept.clone(),
                ))
                .add_service(FlowTypeServiceServer::with_interceptor(
                    flow_type_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeFunctionDefinitionServiceServer::with_interceptor(
                    runtime_function_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeStatusServiceServer::with_interceptor(
                    runtime_status_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        } else {
            Server::builder()
                .add_service(DataTypeServiceServer::with_interceptor(
                    data_type_server,
                    intercept.clone(),
                ))
                .add_service(FlowTypeServiceServer::with_interceptor(
                    flow_type_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeFunctionDefinitionServiceServer::with_interceptor(
                    runtime_function_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeStatusServiceServer::with_interceptor(
                    runtime_status_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        }
    }
}
