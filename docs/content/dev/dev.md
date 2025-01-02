---
title: Develop Aquila
---

# Develop Aquila

## Requirements

To contribute to Aquila's development, you need the following expertise and tools:

- **Advanced Rust Knowledge**: Proficient in Rust programming, including complex abstractions and performance optimizations.
- **Experience with gRPC**: Familiarity with building and consuming gRPC services.
- **Redis & RabbitMQ**: Understanding of setting up and using Redis and RabbitMQ for messaging and caching.

---

## Setting Up a Local Development Environment

Choose between a manual setup or using a pre-configured development container.

### Manual Setup

1. **Install Rust and Cargo**  
   Install the latest versions of Rust and its package manager, Cargo. Use [Rustup](https://rustup.rs/) for an easy installation.
2. **Set Up Local Redis and RabbitMQ Services**
    - Install Redis and RabbitMQ on your local machine.
    - Ensure both services are running and accessible.

### Using a Development Container

1. Set up a development container with all required dependencies pre-installed (details to be provided).
2. Ensure your container runtime (e.g., Docker) is installed and running.
3. Open the project in a compatible IDE like Visual Studio Code, and select the dev-container configuration to start development.

---

## Additional Notes

- Ensure all dependencies are compatible with the version of Aquila you are working on.
- Use the provided `.env` file as a reference for setting up your environment variables.

---