use std::env;
use std::f32::consts::E;
use std::fmt::{Debug, Display};
use std::str::FromStr;
use log::{error, info, log};
use crate::configuration::environment::Environment;

/// Struct for all relevant `Aquila` startup configurations
pub struct Config {
    /// Aquila environment
    ///
    /// Options:
    /// `development` (default)
    /// `staging`
    /// `production`
    pub environment: Environment,

    /// URL to the Redis Server.
    /// Default none
    pub redis_url: String,

    /// If enabled `Aquila` will update its flows on a scheduled basis.
    /// Default: false
    pub enable_scheduled_update: bool,

    /// Interval for `Aquila` to ask `Sagittarius` about updated flows.
    /// Unit: `Seconds`
    /// Default: 3600 seconds => 1 hour
    pub update_schedule_interval: u32,

    /// If enabled `Aquila` will create a gRPC Stream to `Sagittarius`
    /// `Sagittarius` will then tell `Aquila` when a Flow has been updated.
    /// Default: true
    pub enable_grpc_update: bool,

    /// Fallback file to load flows if gRPC & scheduling is disabled.
    pub flow_fallback_path: String,

    /// Verification Token required for internal communication
    pub session_token: String,

    /// URL to the `Sagittarius` Server.
    pub backend_url: String,
}

/// Implementation for all relevant `Aquila` startup configurations
///
/// Behavior:
/// Searches for the env. file at root level. Filename: `.env`
impl Config {
    pub fn new() -> Self {
        let result = dotenv::from_filename(".env");
        match result {
            Ok(_) => info!(".env file loaded successfully"),
            Err(e) => error!("Error loading .env file: {}", e),
        }

        Config {
            environment: Self::get_environment("ENVIRONMENT", Environment::Development),
            redis_url: Self::get_string("REDIS_URL", "redis://redis:6379"),
            enable_scheduled_update: Self::get_bool("ENABLE_SCHEDULED_UPDATE", false),
            update_schedule_interval: Self::get_u32("UPDATE_SCHEDULE_INTERVAL", 3600),
            flow_fallback_path: Self::get_string("FLOW_FALLBACK_PATH", "configuration/configuration.json"),
            enable_grpc_update: Self::get_bool("ENABLE_GRPC_UPDATE", false),
            session_token: Self::get_string("SESSION_TOKEN", "default_session_token"),
            backend_url: Self::get_string("BACKEND_URL", "http://localhost:8080"),
        }
    }

    fn get_environment(key: &str, default: Environment) -> Environment {
        let value = match env::var(key) {
            Ok(result) => {
                info!("Env. {} was found", key);
                result
            }
            Err(_) => {
                error!("Env. {} was not found", key);
                return default;
            }
        };

        Environment::from_str(&value)
    }

    fn get_string(key: &str, default: &str) -> String {
        Self::get_env_with_default(key, String::from(default))
    }

    fn get_bool(key: &str, default: bool) -> bool {
        Self::get_env_with_default(key, default)
    }
    fn get_u32(key: &str, default: u32) -> u32 {
        Self::get_env_with_default(key, default)
    }

    pub fn get_env_with_default<T>(name: &str, default: T) -> T
    where
        T: FromStr,
        T: Display,
        T: Debug,
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

        info!("Env. variable {} was set to the value: {:?}", name, result);
        result
    }
}