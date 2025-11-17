---
title: Aquila Development Guide
---

Follow this guide to develop Aquila.

## Requirements

To contribute to Aquila's development, you need the following expertise and tools:

- **Rust Knowledge**: Proficient in Rust programming, including complex abstractions and performance optimizations.
- **Experience with gRPC**: Familiarity with building and consuming gRPC services.
- **NATS**: Understand Pub/Sub and JetStream.

---

## Setting Up a Local Development Environment

1. **Install Rust and Cargo**  
   Install the latest versions of Rust and its package manager, Cargo. Use [Rustup](https://rustup.rs/) for an easy installation.
2. **Set Up Local NATS Instance**
    - Install NATS on your local machine or use the Dockerimage.
    - Ensure its running and accessible for Aquila.
    - Activate JetStream
    - For any help refer to the [NATS documentation](https://docs.nats.io/running-a-nats-service/introduction/installation)

---

## Additional Notes

- Ensure all dependencies are compatible with the version of Aquila you are working on.
- Use the provided `.env` file as a reference for setting up your environment variables.

---