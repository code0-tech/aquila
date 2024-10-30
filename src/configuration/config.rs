use std::env;
use std::str::FromStr;
use log::{error, info};
use crate::configuration::environment::Environment;

pub struct Config {
    pub environment: Environment,
    pub redis_url: String,
    pub enable_scheduled_update: bool,
    pub update_schedule_interval: i64,
    pub enable_grpc_update: bool,
    pub session_token: String,
    pub backend_url: String,
    pub rabbitmq_url: String,
    pub rabbitmq_user: String,
    pub rabbitmq_password: String,
}

impl Config {
    pub fn new() -> Self {
        let result = dotenv::from_filename(".env");
        match result {
            Ok(_) => info!(".env file loaded successfully"),
            Err(e) => error!("Error loading .env file: {}", e),
        }

        Config {
            environment: Self::get_environment("ENVIRONMENT", "development"),
            redis_url: Self::get_string("REDIS_URL", "redis://redis:6379"),
            enable_scheduled_update: Self::get_bool("ENABLE_SCHEDULED_UPDATE", false),
            update_schedule_interval: Self::get_i64("UPDATE_SCHEDULE_INTERVAL", 3600),
            enable_grpc_update: Self::get_bool("ENABLE_GRPC_UPDATE", false),
            session_token: Self::get_string("SESSION_TOKEN", "default_session_token"),
            backend_url: Self::get_string("BACKEND_URL", "http://localhost:8080"),
            rabbitmq_url: Self::get_string("RABBITMQ_URL", "amqp://localhost:5672"),
            rabbitmq_user: Self::get_string("RABBITMQ_USER", "guest"),
            rabbitmq_password: Self::get_string("RABBITMQ_PASSWORD", "guest"),
        }
    }

    fn get_environment(key: &str, default: &str) -> Environment {
        let value = match env::var(key) {
            Ok(result) => {
                info!("Env. {} was found", key);
                result
            }
            Err(_) => {
                error!("Env. {} was not found", key);
                default
            }.parse().unwrap()
        };

        Environment::from_str(&value)
    }

    fn get_string(key: &str, default: &str) -> String {
        Self::get_env_with_default(key, String::from(default))
    }

    fn get_bool(key: &str, default: bool) -> bool {
        Self::get_env_with_default(key, default)
    }

    fn get_i64(key: &str, default: i64) -> i64 {
        Self::get_env_with_default(key, default)
    }

    pub fn get_env_with_default<T>(name: &str, default: T) -> T
    where
        T: FromStr,
    {
        let env_variable = match env::var(name) {
            Ok(result) => {
                error!("Env. Variable {name} found.");
                result
            }
            Err(find_error) => {
                error!("Env. Variable {name} wasn't found. Reason: {find_error}");
                return default;
            }
        };

        match env_variable.parse::<T>() {
            Ok(parsed_result) => {
                error!("Env. Variable {name} was successfully parsed.");
                parsed_result
            }
            Err(_) => {
                error!("Env. Variable {name} wasn't parsable.");
                default
            }
        }
    }
}