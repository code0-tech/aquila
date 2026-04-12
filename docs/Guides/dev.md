---
title: Aquila Development Guide
---

Follow this guide to develop Aquila.

## Requirements

To contribute to Aquila's development, you need the following expertise and tools:

- **[Rust Knowledge](https://rust-lang.org/)**
- **[Experience with gRPC](https://grpc.io)**
- **[NATS](https://nats.io)**

---

## Setting Up a Virtual Development Environment (Prefered)

[Visit Setup Guide](https://docs.code0.tech/general/install/)

Use the Docker-Compose to start our application. Make shure to stop Aquila for development. Alternatively `COMPOSE_PROFILES` can be set to only `ide` to exclude all runtimes services (manual NATS start is required)

## Setting Up a Local Development Environment

1. **Install Rust and Cargo**  
   Install the latest versions of Rust and its package manager, Cargo. Use [Rustup](https://rustup.rs/) for an easy installation.
2. **Set Up Local NATS Instance**
    - Install NATS on your local machine or use the Dockerimage.
    - Ensure its running and accessible for Aquila.
    - Activate JetStream
    - For any help refer to the [NATS documentation](https://docs.nats.io/running-a-nats-service/introduction/installation)
3. **Setup Sagittarius**
    - Ensure Sagittarius gRPC is running and accessible for Aquila.
    - [Repository](https://github.com/code0-tech/sagittarius)

---

## Additional Notes

- Ensure all dependencies are compatible with the version of Aquila you are working on.
- Use the provided `.env` file as a reference for setting up your environment variables.

---
