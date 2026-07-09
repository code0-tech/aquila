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
pub mod telemetry;

const CONFIG_PATH_ENV: &str = "AQUILA_CONFIG_PATH";
const SERVICE_CONFIG_PATH_ENV: &str = "AQUILA_SERVICE_CONFIG_PATH";

#[tokio::main]
async fn main() {
    // Load .env before config-rs applies environment overrides.
    load_env_file();
    let config_result = match std::env::var(CONFIG_PATH_ENV) {
        Ok(path) => AquilaConfig::try_from_path(path),
        Err(_) => AquilaConfig::try_new(),
    };

    let log_level = config_result
        .as_ref()
        .map(|config| config.log_level.as_str())
        .unwrap_or("debug");
    let telemetry_config = config_result
        .as_ref()
        .map(|config| config.opentelemetry.clone())
        .unwrap_or_default();
    let environment = config_result
        .as_ref()
        .map(|config| config.environment.to_string())
        .unwrap_or_else(|_| "unknown".into());
    let telemetry_config = telemetry::OpenTelemetry {
        enabled: telemetry_config.enabled,
        service_name: telemetry_config.service_name,
        logs_endpoint: telemetry_config.logs_endpoint,
        metrics_endpoint: telemetry_config.metrics_endpoint,
        traces_endpoint: telemetry_config.traces_endpoint,
    };
    let telemetry = telemetry::Telemetry::initialize(
        &telemetry_config,
        telemetry::TelemetrySettings {
            environment: &environment,
            default_log_level: log_level,
            service_version: env!("CARGO_PKG_VERSION"),
            instrumentation_name: env!("CARGO_PKG_NAME"),
            initialize_metrics: Some(telemetry::metrics::initialize),
        },
    )
    .unwrap_or_else(|error| panic!("failed to initialize telemetry: {error}"));
    install_panic_logging();

    let config = config_result
        .unwrap_or_else(|error| panic!("failed to load Aquila configuration: {error}"));
    log::info!("Starting Aquila runtime gateway");

    let app_readiness = AppReadiness::new();
    let service_config = std::env::var_os(SERVICE_CONFIG_PATH_ENV)
        .map(ServiceConfiguration::from_path)
        .unwrap_or_default();
    log::debug!("{config}");

    startup::run(config, app_readiness, service_config).await;
    telemetry.shutdown();
}

fn install_panic_logging() {
    std::panic::set_hook(Box::new(move |panic_info| {
        let message = if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
            *message
        } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
            message.as_str()
        } else {
            "<non-string panic payload>"
        };

        let location = panic_info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".into());
        telemetry::errors::panic(message, &location);
    }));
}
