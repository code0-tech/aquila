//! Clients Aquila uses to talk *to* Sagittarius: flow synchronization,
//! module registration, runtime status forwarding, and the test/live
//! execution stream. See [`retry`] for the shared reconnect-with-backoff
//! logic every long-lived stream client here is built on.

pub mod flow_service_client_impl;
pub mod module_service_client_impl;
pub mod retry;
pub mod runtime_status_service_client_impl;
pub mod test_execution_client_impl;
