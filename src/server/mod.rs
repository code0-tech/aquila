use crate::{
    configuration::{config::Config, service::ServiceConfiguration, state::AppReadiness},
    sagittarius::{
        module_service_client_impl::SagittariusModuleServiceClient,
        runtime_status_service_client_impl::SagittariusRuntimeStatusServiceClient,
        runtime_usage_client_impl::SagittariusRuntimeUsageClient,
    },
    server::{
        action_transfer_service_server_impl::AquilaActionTransferServiceServer,
        module_service_server_impl::AquilaModuleServiceServer,
        runtime_status_service_server_impl::AquilaRuntimeStatusServiceServer,
        runtime_usage_service_server_impl::AquilaRuntimeUsageServiceServer,
    },
};
use async_nats::jetstream::kv::Store;
use log::info;
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tonic::{
    Request, Status,
    transport::{Channel, Server},
};
use tucana::aquila::{
    action_transfer_service_server::ActionTransferServiceServer,
    module_service_server::ModuleServiceServer,
    runtime_status_service_server::RuntimeStatusServiceServer,
    runtime_usage_service_server::RuntimeUsageServiceServer,
};

mod action_transfer_service_server_impl;
mod module_service_server_impl;
mod runtime_status_service_server_impl;
mod runtime_usage_service_server_impl;

pub struct AquilaGRPCServer {
    token: String,
    nats_url: String,
    address: SocketAddr,
    with_health_service: bool,
    app_readiness: AppReadiness,
    channel: Channel,
    service_configuration: ServiceConfiguration,
    nats_client: async_nats::Client,
    kv_store: Arc<Store>,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    is_static: bool,
}

impl AquilaGRPCServer {
    pub fn new(
        config: &Config,
        app_readiness: AppReadiness,
        channel: Channel,
        service_configuration: ServiceConfiguration,
        nats_client: async_nats::Client,
        kv_store: Arc<Store>,
        action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    ) -> Self {
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
            service_configuration,
            nats_client,
            kv_store,
            action_config_tx,
            is_static: config.is_static(),
        }
    }

    pub async fn start(&self) -> Result<(), tonic::transport::Error> {
        let module_service = Arc::new(Mutex::new(SagittariusModuleServiceClient::new(
            self.channel.clone(),
            self.token.clone(),
        )));

        info!("ModuleService started");

        let runtime_usage_service = Arc::new(Mutex::new(SagittariusRuntimeUsageClient::new(
            self.channel.clone(),
            self.token.clone(),
        )));

        info!("RuntimeUsageService started");

        let runtime_status_service = Arc::new(Mutex::new(
            SagittariusRuntimeStatusServiceClient::new(self.channel.clone(), self.token.clone()),
        ));

        info!("RuntimeStatusService started");

        let module_server = AquilaModuleServiceServer::new(
            module_service.clone(),
            self.service_configuration.clone(),
        );
        let runtime_usage_server = AquilaRuntimeUsageServiceServer::new(
            runtime_usage_service.clone(),
            self.service_configuration.clone(),
        );
        let runtime_status_server = AquilaRuntimeStatusServiceServer::new(
            runtime_status_service.clone(),
            self.service_configuration.clone(),
        );

        let action_transfer_server = AquilaActionTransferServiceServer::new(
            self.nats_client.clone(),
            self.kv_store.as_ref().clone(),
            self.service_configuration.clone(),
            self.action_config_tx.clone(),
            self.is_static,
        );

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
                .add_service(ModuleServiceServer::with_interceptor(
                    module_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeUsageServiceServer::with_interceptor(
                    runtime_usage_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeStatusServiceServer::with_interceptor(
                    runtime_status_server,
                    intercept.clone(),
                ))
                .add_service(ActionTransferServiceServer::with_interceptor(
                    action_transfer_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        } else {
            Server::builder()
                .add_service(ModuleServiceServer::with_interceptor(
                    module_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeUsageServiceServer::with_interceptor(
                    runtime_usage_server,
                    intercept.clone(),
                ))
                .add_service(RuntimeStatusServiceServer::with_interceptor(
                    runtime_status_server,
                    intercept.clone(),
                ))
                .add_service(ActionTransferServiceServer::with_interceptor(
                    action_transfer_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        }
    }
}
