//! Aquila's telemetry surface: logging/tracing/error reporting setup comes
//! from the shared `code0_flow` crate, re-exported here so the rest of the
//! codebase depends on `crate::telemetry` rather than reaching into that
//! crate directly. [`metrics`] is Aquila-specific: the OpenTelemetry
//! instruments this service emits.

pub mod metrics;

pub use code0_flow::flow_telemetry::{OpenTelemetry, Telemetry, TelemetrySettings, errors};
