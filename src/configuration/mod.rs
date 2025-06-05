use code0_flow::flow_config::{env_with_default, environment::Environment, mode::Mode};
use log::info;

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

    /// URL to the Redis Server.
    /// Default none
    pub redis_url: String,

    /// Fallback file to load flows if gRPC & scheduling is disabled.
    pub flow_fallback_path: String,

    /// Verification Token required for internal communication
    pub runtime_token: String,

    /// URL to the `Sagittarius` Server.
    pub backend_url: String,
}

/// Implementation for all relevant `Aquila` startup configurations
///
/// Behavior:
/// Searches for the env. file at root level. Filename: `.env`
impl Config {
    pub fn new() -> Self {
        Config {
            environment: env_with_default("ENVIRONMENT", Environment::Development),
            mode: env_with_default("MODE", Mode::STATIC),
            redis_url: env_with_default("REDIS_URL", String::from("redis://localhost:6379")),
            flow_fallback_path: env_with_default(
                "FLOW_FALLBACK_PATH",
                String::from("../flow/test_flow_one.json"),
            ),
            runtime_token: env_with_default("RUNTIME_TOKEN", String::from("default_session_token")),
            backend_url: env_with_default("BACKEND_URL", String::from("http://localhost:8080")),
        }
    }

    pub fn is_static(&self) -> bool {
        self.mode == Mode::STATIC
    }
}
