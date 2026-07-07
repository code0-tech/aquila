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

use crate::configuration::config::OpenTelemetry as TelemetryConfig;

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

        if !config.enabled || !config.has_enabled_exporter() {
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

        let resource = Resource::builder()
            .with_service_name(config.service_name.clone())
            .with_attributes([
                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
                KeyValue::new("deployment.environment.name", environment.to_owned()),
            ])
            .build();

        let tracer_provider = if let Some(endpoint) = config.traces_endpoint() {
            let exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.to_owned())
                .build()?;
            let provider = SdkTracerProvider::builder()
                .with_resource(resource.clone())
                .with_batch_exporter(exporter)
                .build();
            global::set_text_map_propagator(TraceContextPropagator::new());
            Some(provider)
        } else {
            None
        };

        let logger_provider = if let Some(endpoint) = config.logs_endpoint() {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.to_owned())
                .build()?;
            Some(
                SdkLoggerProvider::builder()
                    .with_resource(resource.clone())
                    .with_batch_exporter(exporter)
                    .build(),
            )
        } else {
            None
        };

        let meter_provider = if let Some(endpoint) = config.metrics_endpoint() {
            let exporter = opentelemetry_otlp::MetricExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.to_owned())
                .build()?;
            let reader = PeriodicReader::builder(exporter).build();
            let provider = SdkMeterProvider::builder()
                .with_resource(resource)
                .with_reader(reader)
                .build();
            global::set_meter_provider(provider.clone());
            metrics::initialize();
            Some(provider)
        } else {
            None
        };

        if config.logs_endpoint().is_some() || config.traces_endpoint().is_some() {
            errors::enable_backtraces();
        }

        let trace_layer = tracer_provider.as_ref().map(|provider| {
            tracing_opentelemetry::layer()
                .with_tracer(provider.tracer(env!("CARGO_PKG_NAME")))
                .with_error_records_to_exceptions(true)
                .with_error_events_to_status(true)
                .with_filter(filter_fn(|metadata| {
                    metadata.target() != errors::SUMMARY_TARGET
                }))
        });
        let log_layer = logger_provider.as_ref().map(|provider| {
            OpenTelemetryTracingBridge::new(provider).with_filter(filter_fn(|metadata| {
                metadata.target() != errors::SUMMARY_TARGET
            }))
        });

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(trace_layer)
            .with(log_layer)
            .init();

        Ok(Self {
            logger_provider,
            meter_provider,
            tracer_provider,
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
