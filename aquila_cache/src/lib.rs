use redis::Client;

pub fn build_connection(redis_url: String) -> Client {
    Client::open(redis_url).unwrap_or_else(|err| {
        panic!("Cannot connect to redis instance {err}")
    })
}