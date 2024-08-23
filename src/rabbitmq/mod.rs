use std::sync::Arc;
use rabbitmq_stream_client::Environment;
use tokio::sync::Mutex;
use crate::env::environment::get_env_with_default;

pub async fn init_rabbitmq() -> Arc<Mutex<Box<Environment>>> {
    let host = dotenv::var("RABBITMQ_HOST")
        .expect("Cannot get RabbitMQ host from .env");
    let username = dotenv::var("RABBITMQ_USERNAME")
        .expect("Cannot get RabbitMQ username from .env");
    let password = dotenv::var("RABBITMQ_PASSWORD")
        .expect("Cannot get RabbitMQ password from .env");

    let port = get_env_with_default("RABBITMQ_PORT", 0);
    
    Arc::new(Mutex::new(Box::new(connect(&host, port, &username, &password).await)))
}

async fn connect(host: &str, port: u16, username: &str, password: &str) -> Environment {
    Environment::builder()
        .host(host)
        .port(port)
        .username(username)
        .password(password)
        .build()
        .await
        .expect("Unable to connect to rabbitmq")
}