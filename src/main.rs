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
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();
    install_panic_logging();

    let config = config_result
        .unwrap_or_else(|error| panic!("failed to load Aquila configuration: {error}"));
    log::info!("Starting Aquila");

    let app_readiness = AppReadiness::new();
    let service_config = std::env::var_os(SERVICE_CONFIG_PATH_ENV)
        .map(ServiceConfiguration::from_path)
        .unwrap_or_default();
    log::debug!("{config}");

    startup::run(config, app_readiness, service_config).await;
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

        match panic_info.location() {
            Some(location) => log::error!(
                "Process panic message={} file={} line={} column={}",
                message,
                location.file(),
                location.line(),
                location.column()
            ),
            None => log::error!("Process panic message={} location=unknown", message),
        }
    }));
}
