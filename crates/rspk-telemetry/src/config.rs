//! Telemetry configuration.

use std::env;
use std::time::Duration;

/// Configuration for the OpenTelemetry stack.
///
/// Can be built manually or loaded from environment variables via
/// [`from_env`](Self::from_env).
#[derive(Debug, Clone)]
pub struct TelemetryConfig
{
    /// Whether telemetry is enabled at all.
    pub enabled:      bool,
    /// Whether to export logs via OTLP.
    pub logs_enabled: bool,
    /// OTLP collector endpoint (gRPC).
    pub endpoint:     String,
    /// Export timeout.
    pub timeout:      Duration,
    /// Service name reported to the collector.
    pub service_name: String,
    /// Whether the current CLI invocation runs in dry-run mode.
    pub dry_run:      bool,
}

impl Default for TelemetryConfig
{
    fn default() -> Self
    {
        Self {
            enabled:      false,
            logs_enabled: false,
            endpoint:     "http://localhost:4317".to_string(),
            timeout:      Duration::from_secs(5),
            service_name: "pk".to_string(),
            dry_run:      false,
        }
    }
}

impl TelemetryConfig
{
    /// Loads configuration from environment variables.
    ///
    /// Recognized variables:
    ///
    /// - `PK_TELEMETRY` — `1` / `true` enables telemetry.
    /// - `PK_TELEMETRY_LOGS` — `1` / `true` enables log export.
    /// - `OTEL_EXPORTER_OTLP_ENDPOINT` — collector URL.
    /// - `OTEL_SERVICE_NAME` — service name.
    pub fn from_env() -> Self
    {
        let mut cfg = Self::default();

        if let Ok(v) = env::var("PK_TELEMETRY")
        {
            cfg.enabled = matches!(v.as_str(), "1" | "true" | "yes" | "on");
        }
        if let Ok(v) = env::var("PK_TELEMETRY_LOGS")
        {
            cfg.logs_enabled =
                matches!(v.as_str(), "1" | "true" | "yes" | "on");
        }
        if let Ok(v) = env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        {
            cfg.endpoint = v;
        }
        if let Ok(v) = env::var("OTEL_SERVICE_NAME")
        {
            cfg.service_name = v;
        }
        cfg
    }

    /// Force-enables telemetry (useful for CLI flags).
    pub fn enabled(mut self) -> Self
    {
        self.enabled = true;
        self
    }

    /// Enables OTLP log export.
    pub fn with_logs(mut self) -> Self
    {
        self.logs_enabled = true;
        self
    }

    /// Sets the OTLP collector endpoint.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self
    {
        self.endpoint = endpoint.into();
        self
    }

    /// Sets the dry-run flag (attached as a resource attribute).
    pub fn with_dry_run(mut self, dry_run: bool) -> Self
    {
        self.dry_run = dry_run;
        self
    }
}
