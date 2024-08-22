# Specs

[Rust](https://www.rust-lang.org/) x [Tonic](https://docs.rs/tonic/latest/tonic/)

## Env-Variables

| Name                     | Description                                                                  | Default |
|--------------------------|------------------------------------------------------------------------------|---------|
| SESSION_TOKEN            | Session token for authenticated communication between Aquila and Sagittarius |         |
| BACKEND_HOST             | Hostname of the running Sagittarius                                          |         |
| BACKEND_PORT             | Port of the running Sagittarius instance                                     |         |
| RABBITMQ_HOST            | Hostname of the running RabbitMQ instance                                    |         |
| RABBITMQ_PORT            | Port of the running RabbitMQ instance                                        |         |
| RABBITMQ_USER            | Username of the running RabbitMQ instance                                    |         |
| RABBITMQ_PASSWORD        | Password of the running RabbitMQ instance                                    |         |
| REDIS_URL                | URL for the running Redis instance                                           |         |
| ENABLE_SCHEDULED_UPDATE  | If Aquila should retrieve flows from the Backend                             | true    |
| UPDATE_SCHEDULE_INTERVAL | The given interval in that the flows will be retrieve                        | 180     |
| ENABLE_GRPC_UPDATE       | If the backend should be permitted to send update/deletion request for flows | true    |