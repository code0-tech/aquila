# Specs

[Rust](https://www.rust-lang.org/) x [Tonic](https://docs.rs/tonic/latest/tonic/)
___
# Configuration
## Environment Variables

| Name                       | Description                             | Default                    | Type                          |
|----------------------------|-----------------------------------------|----------------------------|-------------------------------|
| `ENVIRONMENT`              | Specifies the runtime environment       | `development`              | `production` or `development` |
| `REDIS_URL`                | URL for the Redis database              | `'redis://localhost:6379'` | string                        |
| `ENABLE_SCHEDULED_UPDATE`  | Enables periodic updates                | `true`                     | boolean                       |
| `UPDATE_SCHEDULE_INTERVAL` | Interval for updates (in seconds)       | `180`                      | int                           | 
| `ENABLE_GRPC_UPDATE`       | Enables gRPC-based update functionality | `true`                     | boolean                       |
| `SESSION_TOKEN`            | Session token for authentication        | (no default specified)     | string                        |
| `BACKEND_URL`              | Hostname for backend service            | (no default specified)     | string                        |
| `RABBITMQ_URL`             | Port for RabbitMQ                       | (no default specified)     | string                        |
| `RABBITMQ_USER`            | Username for RabbitMQ                   | (no default specified)     | string                        |
| `RABBITMQ_PASSWORD`        | Password for RabbitMQ                   | (no default specified)     | string                        |
