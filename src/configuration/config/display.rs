//! Human-readable rendering of [`Config`] for the startup log line, kept
//! separate from the struct definitions so the config *shape* isn't buried
//! under formatting code. Must stay in sync with the redactions in
//! [`super::DynamicConfig`]'s `Debug` impl — anything secret here needs the
//! same `[FILTERED]` treatment.

use std::fmt;

use super::Config;

impl fmt::Display for Config {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(formatter, "Aquila configuration")?;
        writeln!(formatter, "  Environment: {}", self.environment)?;
        writeln!(formatter, "  Mode:        {}", self.mode)?;
        writeln!(formatter, "  Log level:   {}", self.log_level)?;
        writeln!(formatter, "  OpenTelemetry")?;
        writeln!(formatter, "    Enabled:   {}", self.opentelemetry.enabled)?;
        writeln!(
            formatter,
            "    Service:   {}",
            self.opentelemetry.service_name
        )?;
        writeln!(
            formatter,
            "    Logs:      {}",
            display_optional_url(&self.opentelemetry.logs_endpoint)
        )?;
        writeln!(
            formatter,
            "    Metrics:   {}",
            display_optional_url(&self.opentelemetry.metrics_endpoint)
        )?;
        writeln!(
            formatter,
            "    Traces:    {}",
            display_optional_url(&self.opentelemetry.traces_endpoint)
        )?;
        writeln!(formatter, "  NATS")?;
        writeln!(formatter, "    URL:       {}", self.nats.url)?;
        writeln!(formatter, "    Bucket:    {}", self.nats.bucket)?;
        writeln!(formatter, "  gRPC")?;
        writeln!(
            formatter,
            "    Address:   {}:{}",
            self.grpc.host, self.grpc.port
        )?;
        writeln!(
            formatter,
            "    Health service: {}",
            self.grpc.health_service
        )?;
        writeln!(formatter, "  Static mode")?;
        writeln!(formatter, "    Flow path: {}", self.static_config.flow_path)?;
        writeln!(formatter, "  Dynamic mode")?;
        writeln!(
            formatter,
            "    Backend URL:     {}",
            self.dynamic_config.backend_url
        )?;
        writeln!(formatter, "    Backend token:   [FILTERED]")?;
        writeln!(
            formatter,
            "    Request timeout: {}s",
            self.dynamic_config.backend_unary_timeout_secs
        )?;
        writeln!(formatter, "  Runtime status")?;
        writeln!(
            formatter,
            "    Not responding after: {}s",
            self.runtime_status.not_responding_after_secs
        )?;
        writeln!(
            formatter,
            "    Stopped after:        {}s",
            self.runtime_status.stopped_after_not_responding_secs
        )?;
        writeln!(
            formatter,
            "    Monitor interval:     {}s",
            self.runtime_status.monitor_interval_secs
        )?;
        write!(
            formatter,
            "    Heartbeat interval:   {}m",
            self.runtime_status.heartbeat_interval_minutes
        )
    }
}

fn display_optional_url(url: &Option<String>) -> &str {
    url.as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("<disabled>")
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn display_output_is_readable_and_filters_backend_token() {
        let mut config = Config::default();
        config.dynamic_config.backend_token = "super-secret".into();

        let output = config.to_string();

        assert!(output.starts_with("Aquila configuration\n"));
        assert!(output.contains("  Environment: development"));
        assert!(output.contains("    Address:   127.0.0.1:8081"));
        assert!(output.contains("    Request timeout: 5s"));
        assert!(output.contains("    Backend token:   [FILTERED]"));
        assert!(!output.contains("super-secret"));
        assert!(!output.contains("Config {"));
    }
}
