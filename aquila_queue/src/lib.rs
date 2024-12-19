use std::sync::Arc;
use rabbitmq_stream_client::Environment;
use tokio::sync::Mutex;

pub struct RedisConfiguration {
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl RedisConfiguration {

    pub fn new(host: String, port: u16, username: String, password: String) -> RedisConfiguration {
        Self {host, port, username, password}
    }

}

pub async fn init_rabbitmq(redis_configuration: RedisConfiguration) -> Arc<Mutex<Box<Environment>>> {
    Arc::new(Mutex::new(Box::new(connect(redis_configuration).await)))
}

async fn connect(redis_configuration: RedisConfiguration) -> Environment {
    Environment::builder()
        .host(&*redis_configuration.host)
        .port(redis_configuration.port)
        .username(&*redis_configuration.username)
        .password(&*redis_configuration.password)
        .build()
        .await
        .expect("Unable to connect to rabbitmq")
}