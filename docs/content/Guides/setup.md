---
title: Aquila Setup Guide
---

Follow this guide to set up Aquila.

## Setup Options

### Using Docker

1. **Pull the Docker Image**  
   Pull the latest Docker image from `<image-registry-url>`.
2. **Configure Environment Variables**  
   Set up the necessary environment variables in a `.env` file (see [Environment Variables](#environment-variables)
   section).
3. **Start the Application**  
   Run the Docker container using the appropriate command.

### Manual Installation

1. **Download the Latest Binary**  
   Download the latest Aquila binary from https://github.com/code0-tech/aquila/releases.
2. **Set Up Environment Variables**  
   Configure the `.env` file in the root folder with the required settings.
3. **Ensure Required Services Are Running**
    - **Redis**: Ensure a Redis instance is reachable.
    - **RabbitMQ**: Ensure a RabbitMQ instance is also reachable.
4. **Start the Application**  
   Execute the binary to start Aquila.

---

## Environment Variables

Below is a list of required environment variables for configuring Aquila:

| Name                 | Description                                                                                                                        | Default |
|----------------------|------------------------------------------------------------------------------------------------------------------------------------|---------|
| `MODE`               | Specifies the application mode. `Static`: Startup using a flow file & `Dynamic`: Startup and continuously update from Sagittarius. |         |
| `ENVIRONMENT`        | Defines the application environment for logging and debugging (e.g., `development`, `production`).                                 |         |
| `REDIS_URL`          | The URL of the Redis instance Aquila connects to.                                                                                  |         |
| `BACKEND_URL`        | The URL of the Sagittarius instance Aquila communicates with.                                                                      |         |
| `FLOW_FALLBACK_PATH` | Path to the flow file used for static configuration in `Static` mode.                                                              |         |
| `SESSION_TOKEN`      | A session token for authenticated communication between Aquila and Sagittarius.                                                    |         |

---

### Notes

- Ensure that all required services (Redis, RabbitMQ, Sagittarius) are properly configured and accessible before
  starting Aquila.
- If using Docker, remember to map necessary ports and volumes based on your deployment requirements.

---