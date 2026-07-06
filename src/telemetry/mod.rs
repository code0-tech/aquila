pub mod errors;
pub mod metrics;

use std::error::Error;

use opentelemetry::{KeyValue, global, trace::TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
    trace::SdkTracerProvider,
};
use tracing_subscriber::{
    EnvFilter, Layer, filter::filter_fn, layer::SubscriberExt, util::SubscriberInitExt,
};

use crate::configuration::config::Telemetry as TelemetryConfig;

pub struct Telemetry {
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    tracer_provider: Option<SdkTracerProvider>,
}

impl Telemetry {
    pub fn initialize(
        config: &TelemetryConfig,
        environment: &str,
        default_log_level: &str,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let filter =
            EnvFilter::try_from_default_env().or_else(|_| EnvFilter::try_new(default_log_level))?;
        let fmt_layer = tracing_subscriber::fmt::layer()
            .compact()
            .with_target(false)
            .with_filter(filter_fn(|metadata| {
                metadata.target() != errors::EXCEPTION_TARGET
            }));

        if !config.enabled {
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .init();
            return Ok(Self {
                logger_provider: None,
                meter_provider: None,
                tracer_provider: None,
            });
        }

        errors::enable_backtraces();
        let resource = Resource::builder()
            .with_service_name(env!("CARGO_PKG_NAME"))
            .with_attributes([
                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                KeyValue::new("deployment.environment.name", environment.to_owned()),
            ])
            .build();

        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(config.endpoint.clone())
            .build()?;
        let tracer_provider = SdkTracerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(span_exporter)
            .build();
        let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));

        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(config.endpoint.clone())
            .build()?;
        let logger_provider = SdkLoggerProvider::builder()
            .with_resource(resource.clone())
            .with_batch_exporter(log_exporter)
            .build();

        let metric_exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_endpoint(config.endpoint.clone())
            .build()?;
        let metric_reader = PeriodicReader::builder(metric_exporter).build();
        let meter_provider = SdkMeterProvider::builder()
            .with_resource(resource)
            .with_reader(metric_reader)
            .build();

        global::set_text_map_propagator(TraceContextPropagator::new());
        global::set_meter_provider(meter_provider.clone());
        metrics::initialize();

        let trace_layer = tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_error_records_to_exceptions(true)
            .with_error_events_to_status(true)
            .with_filter(filter_fn(|metadata| {
                metadata.target() != errors::SUMMARY_TARGET
            }));
        let log_layer =
            OpenTelemetryTracingBridge::new(&logger_provider).with_filter(filter_fn(|metadata| {
                metadata.target() != errors::SUMMARY_TARGET
            }));

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(trace_layer)
            .with(log_layer)
            .init();

        Ok(Self {
            logger_provider: Some(logger_provider),
            meter_provider: Some(meter_provider),
            tracer_provider: Some(tracer_provider),
        })
    }

    pub fn shutdown(self) {
        if let Some(provider) = self.logger_provider {
            let _ = provider.shutdown();
        }
        if let Some(provider) = self.meter_provider {
            let _ = provider.shutdown();
        }
        if let Some(provider) = self.tracer_provider {
            let _ = provider.shutdown();
        }
    }
}
