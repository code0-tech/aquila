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
3. **Ensure Required Service Is Running**
   - **NATS**:
     - Ensure a NATS instance is reachable.
     - Activate JetStream
     - For any help refer to the [NATS documentation](https://docs.nats.io/running-a-nats-service/introduction/installation)
   - **Sagittarius**: Ensure a Sagittarius instance is reachable.
4. **Start the Application**  
   Execute the binary to start Aquila.

---

## Environment Variables

Below is a list of required environment variables for configuring Aquila:

| Name                  | Description                                                                                                                        | Default       |
|-----------------------|------------------------------------------------------------------------------------------------------------------------------------|---------------|
| `MODE`                | Specifies the application mode. `Static`: Startup using a flow file & `Dynamic`: Startup and continuously update from Sagittarius. | `static`      |
| `ENVIRONMENT`         | Defines the application environment for logging and debugging (e.g., `development`, `production`).                                 | `developemnt` |
| `NATS_URL`            | The URL of the NATS instance Aquila connects to.                                                                                   | `flow_store`  |
| `NATS_BUCKET`         | The name of the bucket Aquila uses to store flows.                                                                                 |               |
| `SAGITTARIUS_URL`     | The URL of the Sagittarius instance Aquila communicates with.                                                                      |               |
| `FLOW_FALLBACK_PATH`  | Path to the flow file used for static configuration in `Static` mode.                                                              |               |
| `RUNTIME_TOKEN`       | A runtime token for authenticated communication between Aquila and Sagittarius.                                                    |               |
| `GRPC_HOST`           | Hostname for the Aquila instance.                                                                                                  |               |
| `GRPC_PORT`           | gRPC Port for the Aquila instance.                                                                                                 |               |
| `WITH_HEALTH_SERVICE` | If activated Aquila will start with the gRPC-health-service for monitoring.                                                        | `false`       |

---

### Notes

- Ensure that all required services (NATS, Sagittarius) are properly configured and accessible before
  starting Aquila.
- If using Docker, remember to map necessary ports and volumes based on your deployment requirements.

---
