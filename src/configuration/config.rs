use std::{fmt, path::Path};

use config::{Config as ConfigLoader, ConfigError, File};
use serde::{Deserialize, Serialize};

use super::{env::Environment, mode::Mode};

const CONFIG_FILE: &str = "aquila";
const BACKEND_TOKEN_ENV: &str = "AQUILA_BACKEND_TOKEN";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub environment: Environment,
    pub mode: Mode,
    pub log_level: String,
    #[serde(alias = "telemetry")]
    pub opentelemetry: OpenTelemetry,
    pub nats: Nats,
    pub static_config: StaticConfig,
    pub dynamic_config: DynamicConfig,
    pub grpc: Grpc,
    pub runtime_status: RuntimeStatus,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Nats {
    pub url: String,
    pub bucket: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct OpenTelemetry {
    pub enabled: bool,
    pub service_name: String,
    pub logs_endpoint: Option<String>,
    pub metrics_endpoint: Option<String>,
    pub traces_endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct StaticConfig {
    pub flow_path: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DynamicConfig {
    pub backend_url: String,
    pub backend_token: String,
    pub backend_unary_timeout_secs: u64,
}

impl std::fmt::Debug for DynamicConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DynamicConfig")
            .field("backend_url", &self.backend_url)
            .field("backend_token", &"[FILTERED]")
            .field(
                "backend_unary_timeout_secs",
                &self.backend_unary_timeout_secs,
            )
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Grpc {
    pub host: String,
    pub port: u16,
    pub health_service: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct RuntimeStatus {
    pub not_responding_after_secs: u64,
    pub stopped_after_not_responding_secs: u64,
    pub monitor_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            mode: Mode::Static,
            log_level: "debug".into(),
            opentelemetry: OpenTelemetry::default(),
            nats: Nats::default(),
            static_config: StaticConfig::default(),
            dynamic_config: DynamicConfig::default(),
            grpc: Grpc::default(),
            runtime_status: RuntimeStatus::default(),
        }
    }
}

impl Default for OpenTelemetry {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: env!("CARGO_PKG_NAME").into(),
            logs_endpoint: None,
            metrics_endpoint: None,
            traces_endpoint: None,
        }
    }
}

impl OpenTelemetry {
    pub fn logs_endpoint(&self) -> Option<&str> {
        non_empty_url(&self.logs_endpoint)
    }

    pub fn metrics_endpoint(&self) -> Option<&str> {
        non_empty_url(&self.metrics_endpoint)
    }

    pub fn traces_endpoint(&self) -> Option<&str> {
        non_empty_url(&self.traces_endpoint)
    }

    pub fn has_enabled_exporter(&self) -> bool {
        self.logs_endpoint().is_some()
            || self.metrics_endpoint().is_some()
            || self.traces_endpoint().is_some()
    }
}

fn non_empty_url(url: &Option<String>) -> Option<&str> {
    url.as_deref().filter(|value| !value.trim().is_empty())
}

impl Default for Nats {
    fn default() -> Self {
        Self {
            url: "nats://localhost:4222".into(),
            bucket: "flow_store".into(),
        }
    }
}

impl Default for StaticConfig {
    fn default() -> Self {
        Self {
            flow_path: "./flowExport.json".into(),
        }
    }
}

impl Default for DynamicConfig {
    fn default() -> Self {
        Self {
            backend_url: "http://localhost:50051".into(),
            backend_token: "default_session_token".into(),
            backend_unary_timeout_secs: 5,
        }
    }
}

impl Default for Grpc {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8081,
            health_service: false,
        }
    }
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            not_responding_after_secs: 90,
            stopped_after_not_responding_secs: 180,
            monitor_interval_secs: 30,
        }
    }
}

impl Config {
    pub fn new() -> Self {
        Self::try_new()
            .unwrap_or_else(|error| panic!("failed to load Aquila configuration: {error}"))
    }

    pub fn try_new() -> Result<Self, ConfigError> {
        Self::try_from_optional_path(None)
    }

    pub fn try_from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        Self::try_from_optional_path(Some(path.as_ref()))
    }

    fn try_from_optional_path(path: Option<&Path>) -> Result<Self, ConfigError> {
        let mut builder =
            ConfigLoader::builder().add_source(ConfigLoader::try_from(&Self::default())?);

        builder = match path {
            Some(path) => builder.add_source(File::from(path).required(true)),
            None => builder.add_source(File::with_name(CONFIG_FILE).required(false)),
        };

        if let Ok(token) = std::env::var(BACKEND_TOKEN_ENV) {
            builder = builder.set_override("dynamic_config.backend_token", token)?;
        }

        builder.build()?.try_deserialize()
    }

    pub fn is_static(&self) -> bool {
        self.mode == Mode::Static
    }
}

impl fmt::Display for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "Aquila configuration")?;
        writeln!(formatter, "  Environment: {}", self.environment)?;
        writeln!(formatter, "  Mode:        {}", self.mode)?;
        writeln!(formatter, "  Log level:   {}", self.log_level)?;
        writeln!(formatter, "  OpenTelemetry")?;
        writeln!(formatter, "    Enabled:   {}", self.opentelemetry.enabled)?;
        writeln!(
            formatter,
            "    Service:   {}",
            self.opentelemetry.service_name
        )?;
        writeln!(
            formatter,
            "    Logs:      {}",
            display_optional_url(&self.opentelemetry.logs_endpoint)
        )?;
        writeln!(
            formatter,
            "    Metrics:   {}",
            display_optional_url(&self.opentelemetry.metrics_endpoint)
        )?;
        writeln!(
            formatter,
            "    Traces:    {}",
            display_optional_url(&self.opentelemetry.traces_endpoint)
        )?;
        writeln!(formatter, "  NATS")?;
        writeln!(formatter, "    URL:       {}", self.nats.url)?;
        writeln!(formatter, "    Bucket:    {}", self.nats.bucket)?;
        writeln!(formatter, "  gRPC")?;
        writeln!(
            formatter,
            "    Address:   {}:{}",
            self.grpc.host, self.grpc.port
        )?;
        writeln!(
            formatter,
            "    Health service: {}",
            self.grpc.health_service
        )?;
        writeln!(formatter, "  Static mode")?;
        writeln!(formatter, "    Flow path: {}", self.static_config.flow_path)?;
        writeln!(formatter, "  Dynamic mode")?;
        writeln!(
            formatter,
            "    Backend URL:     {}",
            self.dynamic_config.backend_url
        )?;
        writeln!(formatter, "    Backend token:   [FILTERED]")?;
        writeln!(
            formatter,
            "    Request timeout: {}s",
            self.dynamic_config.backend_unary_timeout_secs
        )?;
        writeln!(formatter, "  Runtime status")?;
        writeln!(
            formatter,
            "    Not responding after: {}s",
            self.runtime_status.not_responding_after_secs
        )?;
        writeln!(
            formatter,
            "    Stopped after:        {}s",
            self.runtime_status.stopped_after_not_responding_secs
        )?;
        write!(
            formatter,
            "    Monitor interval:     {}s",
            self.runtime_status.monitor_interval_secs
        )
    }
}

fn display_optional_url(url: &Option<String>) -> &str {
    non_empty_url(url).unwrap_or("<disabled>")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use config::Config as ConfigLoader;

    use super::{Config, OpenTelemetry};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn environment_overrides_backend_token() {
        let _guard = ENV_LOCK.lock().expect("environment test lock poisoned");

        // SAFETY: access to these process-wide variables is serialized for this test.
        unsafe {
            std::env::set_var("AQUILA_BACKEND_TOKEN", "environment-token");
        }

        let config = Config::try_new().expect("configuration should load");

        // SAFETY: access to these process-wide variables is serialized for this test.
        unsafe {
            std::env::remove_var("AQUILA_BACKEND_TOKEN");
        }

        assert_eq!(config.dynamic_config.backend_token, "environment-token");
    }

    #[test]
    fn debug_output_filters_backend_token() {
        let mut config = Config::default();
        config.dynamic_config.backend_token = "super-secret".into();

        let output = format!("{config:#?}");

        assert!(output.contains("[FILTERED]"));
        assert!(!output.contains("super-secret"));
    }

    #[test]
    fn display_output_is_readable_and_filters_backend_token() {
        let mut config = Config::default();
        config.dynamic_config.backend_token = "super-secret".into();

        let output = config.to_string();

        assert!(output.starts_with("Aquila configuration\n"));
        assert!(output.contains("  Environment: development"));
        assert!(output.contains("    Address:   127.0.0.1:8081"));
        assert!(output.contains("    Request timeout: 5s"));
        assert!(output.contains("    Backend token:   [FILTERED]"));
        assert!(!output.contains("super-secret"));
        assert!(!output.contains("Config {"));
    }

    #[test]
    fn opentelemetry_endpoints_are_enabled_by_presence() {
        let config: OpenTelemetry = ConfigLoader::builder()
            .add_source(
                ConfigLoader::try_from(&OpenTelemetry::default())
                    .expect("default telemetry config should serialize"),
            )
            .set_override("enabled", true)
            .expect("enabled override should apply")
            .set_override("service_name", "sagittarius")
            .expect("service name override should apply")
            .set_override("logs_endpoint", "")
            .expect("logs override should apply")
            .set_override("metrics_endpoint", "  ")
            .expect("metrics override should apply")
            .set_override("traces_endpoint", "http://localhost:4317")
            .expect("traces override should apply")
            .build()
            .expect("telemetry config should build")
            .try_deserialize()
            .expect("telemetry config should deserialize");

        assert!(config.enabled);
        assert_eq!(config.service_name, "sagittarius");
        assert_eq!(config.logs_endpoint(), None);
        assert_eq!(config.metrics_endpoint(), None);
        assert_eq!(config.traces_endpoint(), Some("http://localhost:4317"));
        assert!(config.has_enabled_exporter());
    }
}
