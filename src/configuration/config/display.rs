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
        writeln!(
            formatter,
            "    Action transfer concurrency: {}",
            self.grpc.action_transfer_concurrency_limit
        )?;
        writeln!(
            formatter,
            "    Max execution result size: {} bytes",
            self.grpc.max_execution_result_size
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
            "    Heartbeat interval:   {}m",
            self.runtime_status.heartbeat_interval_minutes
        )?;
        writeln!(formatter, "  Runtime usage")?;
        writeln!(
            formatter,
            "    Stream:           {}",
            self.runtime_usage.stream
        )?;
        writeln!(
            formatter,
            "    Forward interval: {}s",
            self.runtime_usage.forward_interval_secs
        )?;
        writeln!(
            formatter,
            "    Batches per request: {}",
            self.runtime_usage.max_batches_per_request
        )?;
        writeln!(formatter, "  Apus")?;
        writeln!(formatter, "    Enabled:    {}", self.apus.enabled)?;
        writeln!(
            formatter,
            "    CA cert:    {}",
            display_optional_path(&self.apus.ca_cert_path)
        )?;
        writeln!(
            formatter,
            "    CA key:     {}",
            display_optional_path(&self.apus.ca_key_path)
        )?;
        writeln!(
            formatter,
            "    Action cert lifetime: {}s",
            self.apus.action_cert_lifetime_secs
        )?;
        writeln!(
            formatter,
            "    Server cert lifetime: {}s",
            self.apus.server_cert_lifetime_secs
        )?;
        write!(
            formatter,
            "    Output dir: {}",
            display_optional_path(&self.apus.output_dir)
        )
    }
}

fn display_optional_url(url: &Option<String>) -> &str {
    url.as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("<disabled>")
}

fn display_optional_path(path: &Option<String>) -> &str {
    path.as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("<not configured>")
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
        assert!(output.contains("    Action transfer concurrency: 256"));
        assert!(output.contains("    Request timeout: 5s"));
        assert!(output.contains("    Stream:           runtime_usage"));
        assert!(output.contains("    Backend token:   [FILTERED]"));
        assert!(!output.contains("super-secret"));
        assert!(!output.contains("Config {"));
    }

    #[test]
    fn display_output_renders_apus_paths_not_configured_by_default() {
        let output = Config::default().to_string();

        assert!(output.contains("  Apus"));
        assert!(output.contains("    Enabled:    false"));
        assert!(output.contains("    CA cert:    <not configured>"));
        assert!(output.contains("    CA key:     <not configured>"));
        assert!(output.contains("    Action cert lifetime: 86400s"));
        assert!(output.contains("    Server cert lifetime: 7776000s"));
        assert!(output.contains("    Output dir: <not configured>"));
    }

    #[test]
    fn display_output_renders_configured_apus_paths() {
        let mut config = Config::default();
        config.apus.enabled = true;
        config.apus.ca_cert_path = Some("/etc/aquila/ca.pem".into());
        config.apus.ca_key_path = Some("/etc/aquila/ca.key".into());

        let output = config.to_string();

        assert!(output.contains("    Enabled:    true"));
        assert!(output.contains("    CA cert:    /etc/aquila/ca.pem"));
        assert!(output.contains("    CA key:     /etc/aquila/ca.key"));
    }
}
