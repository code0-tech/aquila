# Specs

[Rust](https://www.rust-lang.org/) x [Tonic](https://docs.rs/tonic/latest/tonic/)

## Env-Variables

| Name          | Description                                                                  | Notes                 |
|---------------|------------------------------------------------------------------------------|-----------------------|
| SESSION_TOKEN | Session token for authenticated communication between Aquila and Sagittarius |
| BACKEND_HOST  | Hostname of the running Sagittarius                                          |
| BACKEND_PORT  | Port of the running Sagittarius instance                                     | Optional (default: 0) |
| RABBITMQ_HOST | Hostname of the running RabbitMQ instance                                    |
| RABBITMQ_PORT | Port of the running RabbitMQ instance                                        |
| REDIS_HOST    | Hostname of the running Redis instance                                       |
| REDIS_PORT    | Port of the running Redis instance                                           |