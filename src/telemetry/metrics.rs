use std::sync::OnceLock;

use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram, UpDownCounter},
};

static METRICS: OnceLock<Metrics> = OnceLock::new();

struct Metrics {
    flow_operations: Counter<u64>,
    action_connections: Counter<u64>,
    active_actions: UpDownCounter<i64>,
    action_connection_duration: Histogram<f64>,
    action_events: Counter<u64>,
    action_executions: Counter<u64>,
    action_execution_duration: Histogram<f64>,
    action_results: Counter<u64>,
    action_config_updates: Counter<u64>,
    action_failures: Counter<u64>,
}

pub fn initialize() {
    let meter = opentelemetry::global::meter(env!("CARGO_PKG_NAME"));
    let _ = METRICS.set(Metrics {
        flow_operations: meter.u64_counter("aquila.flow.operations").build(),
        action_connections: meter.u64_counter("aquila.action.connections").build(),
        active_actions: meter.i64_up_down_counter("aquila.action.active").build(),
        action_connection_duration: meter
            .f64_histogram("aquila.action.connection.duration")
            .with_unit("s")
            .build(),
        action_events: meter.u64_counter("aquila.action.events").build(),
        action_executions: meter.u64_counter("aquila.action.executions").build(),
        action_execution_duration: meter
            .f64_histogram("aquila.action.execution.duration")
            .with_unit("s")
            .build(),
        action_results: meter.u64_counter("aquila.action.results").build(),
        action_config_updates: meter
            .u64_counter("aquila.action.configuration_updates")
            .build(),
        action_failures: meter.u64_counter("aquila.action.failures").build(),
    });
}

pub fn flow_operation(operation: &'static str, outcome: &'static str, count: u64) {
    if let Some(metrics) = METRICS.get() {
        metrics.flow_operations.add(
            count,
            &[
                KeyValue::new("operation", operation),
                KeyValue::new("outcome", outcome),
            ],
        );
    }
}

fn action_attributes(identifier: &str) -> [KeyValue; 1] {
    [KeyValue::new("action.identifier", identifier.to_owned())]
}

pub fn action_connection(identifier: &str, outcome: &'static str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_connections.add(
            1,
            &[
                KeyValue::new("action.identifier", identifier.to_owned()),
                KeyValue::new("outcome", outcome),
            ],
        );
    }
}

pub fn action_active(identifier: &str, delta: i64) {
    if let Some(metrics) = METRICS.get() {
        metrics
            .active_actions
            .add(delta, &action_attributes(identifier));
    }
}

pub fn action_connection_duration(identifier: &str, seconds: f64) {
    if let Some(metrics) = METRICS.get() {
        metrics
            .action_connection_duration
            .record(seconds, &action_attributes(identifier));
    }
}

pub fn action_event(identifier: &str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_events.add(1, &action_attributes(identifier));
    }
}

pub fn action_execution(identifier: &str, outcome: &'static str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_executions.add(
            1,
            &[
                KeyValue::new("action.identifier", identifier.to_owned()),
                KeyValue::new("outcome", outcome),
            ],
        );
    }
}

pub fn action_result(identifier: &str, outcome: &'static str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_results.add(
            1,
            &[
                KeyValue::new("action.identifier", identifier.to_owned()),
                KeyValue::new("outcome", outcome),
            ],
        );
    }
}

pub fn action_execution_duration(identifier: &str, seconds: f64) {
    if let Some(metrics) = METRICS.get() {
        metrics
            .action_execution_duration
            .record(seconds, &action_attributes(identifier));
    }
}

pub fn action_config_update(identifier: &str, outcome: &'static str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_config_updates.add(
            1,
            &[
                KeyValue::new("action.identifier", identifier.to_owned()),
                KeyValue::new("outcome", outcome),
            ],
        );
    }
}

pub fn action_failure(identifier: &str, reason: &'static str) {
    if let Some(metrics) = METRICS.get() {
        metrics.action_failures.add(
            1,
            &[
                KeyValue::new("action.identifier", identifier.to_owned()),
                KeyValue::new("reason", reason),
            ],
        );
    }
}
