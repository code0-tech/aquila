use redis::aio::MultiplexedConnection;
use redis::Client;
use testcontainers::{ContainerAsync, GenericImage};
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;

pub fn build_connection(redis_url: String) -> Client {
    Client::open(redis_url).unwrap_or_else(|err| {
        panic!("Cannot connect to redis instance {err}")
    })
}

pub async fn setup_redis_test_container() -> (MultiplexedConnection, ContainerAsync<GenericImage>) {
    let container = GenericImage::new("redis", "7.2.4")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await
        .unwrap();

    let host = container.get_host().await.unwrap();
    let host_port = container.get_host_port_ipv4(6379).await.unwrap();

    let url = format!("redis://{host}:{host_port}");
    let client = Client::open(url.as_ref()).unwrap();
    let connection = client.get_multiplexed_async_connection().await;
    (connection.unwrap(), container)
}
