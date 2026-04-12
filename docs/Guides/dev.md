---
title: Aquila Development Guide
---

Use this guide to set up a development environment for Aquila. Choose either the virtual environment (recommended) or a fully local setup.

## Requirements

To contribute to Aquila, you should be comfortable with:

- **[Rust](https://rust-lang.org/)**
- **[gRPC](https://grpc.io)**
- **[NATS](https://nats.io)**

---

## Setting Up a Virtual Development Environment (Preferred)

[Visit Setup Guide](https://docs.code0.tech/general/install/)

Use Docker Compose to start the application. Make sure Aquila is stopped while you are developing locally.  
Alternatively, set `COMPOSE_PROFILES=ide` to exclude runtime services (you will need to start NATS manually).

## Setting Up a Local Development Environment

1. **Install Rust and Cargo**  
   Install the latest versions of Rust and its package manager, Cargo. Use [Rustup](https://rustup.rs/) for an easy installation.
2. **Set Up Local NATS Instance**
    - Install NATS locally or use a Docker image.
    - Ensure it is running and reachable by Aquila.
    - Enable JetStream.
    - For help, refer to the [NATS documentation](https://docs.nats.io/running-a-nats-service/introduction/installation).
3. **Set Up Sagittarius**
    - Ensure Sagittarius gRPC is running and reachable by Aquila.
    - [Repository](https://github.com/code0-tech/sagittarius)

---

## Additional Notes

- Ensure all dependencies are compatible with the Aquila version you are working on.
- Use the provided `.env` file as a reference for setting up your environment variables.

---
