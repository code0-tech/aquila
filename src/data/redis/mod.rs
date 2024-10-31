use redis::Client;

pub fn build_connection() -> Client {
    
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL env not found");

    Client::open(redis_url.to_string()).unwrap_or_else(|err| {
        panic!("Cannot connect to redis instance {redis_url}: {err}")
    })
}