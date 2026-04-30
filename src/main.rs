use crate::configuration::{
    config::Config as AquilaConfig, service::ServiceConfiguration, state::AppReadiness,
};
use code0_flow::flow_config::load_env_file;

pub mod authorization;
pub mod configuration;
pub mod flow;
pub mod sagittarius;
pub mod server;
pub mod startup;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();
    log::info!("Starting Aquila");

    // Load environment variables from .env file
    load_env_file();
    let config = AquilaConfig::new();
    let app_readiness = AppReadiness::new();
    let service_config = ServiceConfiguration::from_path(&config.service_config_path);
    log::debug!(
        "Configuration loaded mode={:?} environment={:?} grpc={}:{} health_service={}",
        config.mode,
        config.environment,
        config.grpc_host,
        config.grpc_port,
        config.with_health_service
    );

    startup::run(config, app_readiness, service_config).await;
}
