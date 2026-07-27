//! Aquila's gRPC surface: the services actions and runtimes connect to.
//! [`dynamic_server`] and [`static_server`] assemble the actual service set
//! for each run mode from the individual `*_service_server_impl` modules.

mod action_transfer;
mod interceptor;
mod module_service_server_impl;
mod runtime_execution_service_server_impl;
mod runtime_status_service_server_impl;

pub mod dynamic_server;
pub mod static_server;

pub use interceptor::create_readiness_interceptor;
