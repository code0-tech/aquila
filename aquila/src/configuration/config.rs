use crate::configuration::environment::Environment;
use crate::configuration::mode::Mode;
use dotenv::from_filename;
use log::{error, info};
use std::env;
use std::str::FromStr;

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
        let result = from_filename("../../../.env");
        match result {
            Ok(_) => info!(".env file loaded successfully"),
            Err(e) => error!("Error loading .env file: {}", e),
        }

        Config {
            environment: Self::get_environment("ENVIRONMENT", Environment::Development),
            mode: Self::get_mode("MODE", Mode::STATIC),
            redis_url: Self::get_string("REDIS_URL", "redis://redis:6379"),
            flow_fallback_path: Self::get_string(
                "FLOW_FALLBACK_PATH",
                "configuration/configuration.json",
            ),
            runtime_token: Self::get_string("RUNTIME_TOKEN", "default_session_token"),
            backend_url: Self::get_string("BACKEND_URL", "http://localhost:8080"),
        }
    }

    fn get_environment(key: &str, default: Environment) -> Environment {
        Self::get_env_with_default(key, default)
    }

    fn get_mode(key: &str, default: Mode) -> Mode {
        Self::get_env_with_default(key, default)
    }

    fn get_string(key: &str, default: &str) -> String {
        Self::get_env_with_default(key, String::from(default))
    }

    pub fn get_env_with_default<T>(name: &str, default: T) -> T
    where
        T: FromStr,
    {
        let env_variable = match env::var(name) {
            Ok(result) => {
                info!("Env. Variable {name} found.");
                result
            }
            Err(find_error) => {
                error!("Env. Variable {name} wasn't found. Reason: {find_error}");
                return default;
            }
        };

        let result = match env_variable.parse::<T>() {
            Ok(parsed_result) => {
                info!("Env. Variable {name} was successfully parsed.");
                parsed_result
            }
            Err(_) => {
                error!("Env. Variable {name} wasn't parsable.");
                default
            }
        };

        info!("Env. variable {} was set to the value", name);
        result
    }
}
