use code0_flow::flow_config::{env_with_default, environment::Environment, mode::Mode};

/// Struct for all relevant `Aquila` startup configurations
pub struct Config {
    /// Aquila environment
    ///
    /// Options:
    /// `development` (default)
    /// `staging`
    /// `production`
    pub environment: Environment,

    /// Aquila mode
    ///
    /// Options:
    /// `static` (default)
    /// `hybrid`
    pub mode: Mode,

    /// URL to the NATS Server.
    pub nats_url: String,

    /// Name of the NATS Bucket.
    pub nats_bucket: String,

    /// Fallback file to load flows if gRPC & scheduling is disabled.
    pub flow_fallback_path: String,

    /// Verification Token required for internal communication
    pub runtime_token: String,

    /// URL to the `Sagittarius` Server.
    pub backend_url: String,

    // Port of the `Aquila` Server
    pub grpc_port: u16,

    // Host of the `Aquila` Server
    pub grpc_host: String,

    pub with_health_service: bool,

    pub action_config_path: String,
}

/// Implementation for all relevant `Aquila` startup configurations
///
/// Behavior:
/// Searches for the env. file at root level. Filename: `.env`
impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub fn new() -> Self {
        Config {
            environment: env_with_default("ENVIRONMENT", Environment::Development),
            mode: env_with_default("MODE", Mode::STATIC),
            nats_url: env_with_default("NATS_URL", String::from("nats://localhost:4222")),
            nats_bucket: env_with_default("NATS_BUCKET", String::from("flow_store")),
            flow_fallback_path: env_with_default(
                "FLOW_FALLBACK_PATH",
                String::from("./flowExport.json"),
            ),
            grpc_port: env_with_default("GRPC_PORT", 8081),
            grpc_host: env_with_default("GRPC_HOST", String::from("127.0.0.1")),
            with_health_service: env_with_default("WITH_HEALTH_SERVICE", false),
            runtime_token: env_with_default("RUNTIME_TOKEN", String::from("default_session_token")),
            backend_url: env_with_default(
                "SAGITTARIUS_URL",
                String::from("http://localhost:50051"),
            ),
            action_config_path: env_with_default("ACTION_CONFIG_PATH", String::from("./action.configuration.json"))
        }
    }

    pub fn is_static(&self) -> bool {
        self.mode == Mode::STATIC
    }
}
