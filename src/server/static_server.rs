use crate::{
    configuration::{config::Config, service::ServiceConfiguration, state::AppReadiness},
    server::{
        action_transfer_service_server_impl::AquilaActionTransferServiceServer,
        create_readiness_interceptor,
    },
};
use async_nats::jetstream::kv::Store;
use log::info;
use std::{net::SocketAddr, sync::Arc};
use tonic::transport::Server;
use tucana::aquila::action_transfer_service_server::ActionTransferServiceServer;

pub struct AquilaStaticServer {
    nats_url: String,
    address: SocketAddr,
    with_health_service: bool,
    app_readiness: AppReadiness,
    service_configuration: ServiceConfiguration,
    nats_client: async_nats::Client,
    kv_store: Arc<Store>,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
}

impl AquilaStaticServer {
    pub fn new(
        config: &Config,
        app_readiness: AppReadiness,
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

        AquilaStaticServer {
            nats_url: config.nats_url.clone(),
            with_health_service: config.with_health_service,
            address,
            app_readiness,
            service_configuration,
            nats_client,
            kv_store,
            action_config_tx,
        }
    }

    pub async fn start(&self) -> Result<(), tonic::transport::Error> {
        let action_transfer_server = AquilaActionTransferServiceServer::new(
            self.nats_client.clone(),
            self.kv_store.as_ref().clone(),
            self.service_configuration.clone(),
            self.action_config_tx.clone(),
            true,
        );

        info!("Starting static gRPC Server...");

        let readiness: Arc<AppReadiness> = Arc::new(self.app_readiness.clone());
        let intercept = create_readiness_interceptor(readiness.clone(), "sagittarius");

        if self.with_health_service {
            info!("Starting with HealthService");
            let health_service = code0_flow::flow_health::HealthService::new(self.nats_url.clone());

            Server::builder()
                .add_service(tonic_health::pb::health_server::HealthServer::new(
                    health_service,
                ))
                .add_service(ActionTransferServiceServer::with_interceptor(
                    action_transfer_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        } else {
            Server::builder()
                .add_service(ActionTransferServiceServer::with_interceptor(
                    action_transfer_server,
                    intercept.clone(),
                ))
                .serve(self.address)
                .await
        }
    }
}
