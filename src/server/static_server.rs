//! Assembles and serves the gRPC service set used in static mode: only
//! action transfer, since there's no Sagittarius to register modules with
//! or report runtime status to.

use crate::{
    ca::CertificateAuthority,
    configuration::{config::Config, service::ServiceConfiguration, state::AppReadiness},
    server::{
        action_transfer::{
            ActionFlowExecutionRegistry, ActionShardRegistry, ActionTransferContext,
            AquilaActionTransferServiceServer,
        },
        apus_certificate::{AquilaCertificateServiceServer, CertificateServiceServer},
        create_readiness_interceptor,
    },
};
use async_nats::jetstream::kv::Store;
use log::info;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tonic::transport::Server;
use tucana::aquila::action_transfer_service_server::ActionTransferServiceServer;

/// Every collaborator `AquilaStaticServer` needs that isn't derived from
/// [`Config`] itself, bundled into one value (mirrors
/// `dynamic_server::DynamicServerDependencies`) so adding one more is a
/// field here instead of another constructor parameter.
pub struct StaticServerDependencies {
    pub app_readiness: AppReadiness,
    pub service_configuration: ServiceConfiguration,
    pub nats_client: async_nats::Client,
    pub kv_store: Arc<Store>,
    pub action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    pub action_flow_tx: tokio::sync::broadcast::Sender<crate::flow::FlowChange>,
    /// `Some` only when `config.apus.enabled` - see [`crate::startup::static_mode`].
    pub apus_ca: Option<Arc<CertificateAuthority>>,
}

pub struct AquilaStaticServer {
    nats_url: String,
    address: SocketAddr,
    with_health_service: bool,
    app_readiness: AppReadiness,
    service_configuration: ServiceConfiguration,
    nats_client: async_nats::Client,
    kv_store: Arc<Store>,
    action_config_tx: tokio::sync::broadcast::Sender<tucana::shared::ModuleConfigurations>,
    action_flow_tx: tokio::sync::broadcast::Sender<crate::flow::FlowChange>,
    action_transfer_concurrency_limit: usize,
    apus_ca: Option<Arc<CertificateAuthority>>,
    apus_action_cert_lifetime: Duration,
    apus_server_cert_lifetime: Duration,
}

impl AquilaStaticServer {
    pub fn new(config: &Config, deps: StaticServerDependencies) -> Self {
        let StaticServerDependencies {
            app_readiness,
            service_configuration,
            nats_client,
            kv_store,
            action_config_tx,
            action_flow_tx,
            apus_ca,
        } = deps;

        let address = match format!("{}:{}", config.grpc.host, config.grpc.port).parse() {
            Ok(addr) => {
                info!("Listening on {:?}", &addr);
                addr
            }
            Err(e) => panic!("Failed to parse address: {:?}", e),
        };

        AquilaStaticServer {
            nats_url: config.nats.url.clone(),
            with_health_service: config.grpc.health_service,
            address,
            app_readiness,
            service_configuration,
            nats_client,
            kv_store,
            action_config_tx,
            action_flow_tx,
            action_transfer_concurrency_limit: config.grpc.action_transfer_concurrency_limit,
            apus_ca,
            apus_action_cert_lifetime: Duration::from_secs(config.apus.action_cert_lifetime_secs),
            apus_server_cert_lifetime: Duration::from_secs(config.apus.server_cert_lifetime_secs),
        }
    }

    pub async fn start(&self) -> Result<(), tonic::transport::Error> {
        let action_transfer_server =
            AquilaActionTransferServiceServer::new(ActionTransferContext {
                client: self.nats_client.clone(),
                kv: self.kv_store.as_ref().clone(),
                actions: self.service_configuration.clone(),
                module_service: None,
                action_config_tx: self.action_config_tx.clone(),
                action_flow_tx: self.action_flow_tx.clone(),
                // Static mode has no ExecutionService for a runtime to report results
                // to, so an action-triggered flow execution can never resolve here -
                // this registry only exists to satisfy the shared context shape.
                flow_execution_registry: ActionFlowExecutionRegistry::new(),
                shard_registry: ActionShardRegistry::new(),
                is_static: true,
                concurrency_limit: self.action_transfer_concurrency_limit,
            });

        info!("Starting static gRPC Server...");

        let readiness: Arc<AppReadiness> = Arc::new(self.app_readiness.clone());
        let intercept = create_readiness_interceptor(readiness.clone(), "sagittarius");

        // Not behind the Sagittarius readiness interceptor: issuing a
        // certificate has no Sagittarius dependency at all, and static
        // mode has no Sagittarius to begin with.
        let apus_certificate_service = self.apus_ca.clone().map(|ca| {
            info!("ApusCertificateService started");
            CertificateServiceServer::new(AquilaCertificateServiceServer::new(
                ca,
                self.service_configuration.clone(),
                self.apus_action_cert_lifetime,
                self.apus_server_cert_lifetime,
            ))
        });

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
                .add_optional_service(apus_certificate_service)
                .serve(self.address)
                .await
        } else {
            Server::builder()
                .add_service(ActionTransferServiceServer::with_interceptor(
                    action_transfer_server,
                    intercept.clone(),
                ))
                .add_optional_service(apus_certificate_service)
                .serve(self.address)
                .await
        }
    }
}
