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

pub enum Environment {
    Development,
    Production,
}