//! OpenTelemetry integration for the `pk` meta package manager.
//!
//! Provides traces, metrics and structured logs that can be exported
//! to any OTEL-compatible backend (Prometheus, Grafana Tempo/Loki,
//! Jaeger, Datadog, Sentry, etc.) via OTLP.
//!
//! # Configuration
//!
//! Telemetry is **disabled by default** to keep the CLI fast and
//! silent. Enable it explicitly via [`TelemetryConfig::enabled`], or
//! use the standard `OTEL_*` environment variables:
//!
//! | Variable | Effect |
//! |----------|--------|
//! | `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP collector URL (default `http://localhost:4317`) |
//! | `OTEL_SERVICE_NAME` | Service name (default `pk`) |
//! | `PK_TELEMETRY=1` | Force enable telemetry |
//! | `PK_TELEMETRY_LOGS=1` | Send logs via OTLP |

#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![allow(clippy::unused_async)]
#![allow(clippy::unused_async_trait_impl)]

mod config;
mod metrics;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_semantic_conventions::resource::{
    SERVICE_NAME, SERVICE_VERSION,
};
use thiserror::Error;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

pub use config::TelemetryConfig;
pub use metrics::{Metrics, Timer};

/// Errors that can occur during telemetry initialization or shutdown.
#[derive(Debug, Error)]
pub enum TelemetryError
{
    /// Failed to build the OTLP trace exporter.
    #[error("trace exporter error: {0}")]
    TraceExporter(String),

    /// Failed to build the OTLP metrics exporter.
    #[error("metrics exporter error: {0}")]
    MetricsExporter(String),

    /// Failed to build the OTLP logs exporter.
    #[error("logs exporter error: {0}")]
    LogsExporter(String),
}

/// RAII guard that shuts down all OTEL providers on drop.
///
/// This is critical for a CLI tool: without an explicit shutdown the
/// last batch of spans/metrics may never reach the collector.
#[derive(Debug)]
pub struct TelemetryGuard
{
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider:  Option<SdkMeterProvider>,
    logger_provider: Option<SdkLoggerProvider>,
}

impl TelemetryGuard
{
    /// Flushes and shuts down all providers, returning any errors.
    ///
    /// Called automatically on drop, but you may call it manually to
    /// capture shutdown errors.
    pub fn shutdown(mut self) -> Result<(), TelemetryError>
    {
        self.shutdown_internal()
    }

    fn shutdown_internal(&mut self) -> Result<(), TelemetryError>
    {
        if let Some(tp) = self.tracer_provider.take()
        {
            if let Err(e) = tp.shutdown()
            {
                return Err(TelemetryError::TraceExporter(format!("{:?}", e)));
            }
        }
        if let Some(mp) = self.meter_provider.take()
        {
            if let Err(e) = mp.shutdown()
            {
                return Err(TelemetryError::MetricsExporter(format!("{:?}", e)));
            }
        }
        if let Some(lp) = self.logger_provider.take()
        {
            if let Err(e) = lp.shutdown()
            {
                return Err(TelemetryError::LogsExporter(format!("{:?}", e)));
            }
        }
        Ok(())
    }
}

impl Drop for TelemetryGuard
{
    fn drop(&mut self)
    {
        let _ = self.shutdown_internal();
    }
}

/// Initializes the full observability stack and returns a guard.
///
/// The returned [`TelemetryGuard`] must outlive the rest of the
/// application — when it is dropped, all buffered telemetry is
/// flushed to the collector.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = rspk_telemetry::TelemetryConfig::from_env();
/// let _guard = rspk_telemetry::init(config)?;
///
/// tracing::info!("pk started");
/// // ... application logic ...
/// # Ok(())
/// # }
/// ```
pub fn init(config: TelemetryConfig) -> Result<TelemetryGuard, TelemetryError>
{
    let resource = Resource::builder()
        .with_attribute(KeyValue::new(
            SERVICE_NAME,
            config.service_name.clone(),
        ))
        .with_attribute(KeyValue::new(
            SERVICE_VERSION,
            env!("CARGO_PKG_VERSION"),
        ))
        .with_attribute(KeyValue::new("pk.dry_run", config.dry_run))
        .build();

    // ── tracing layer (stderr, always on) ──────────────────────
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,rspk=debug"));
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_filter(env_filter);

    let registry = tracing_subscriber::registry().with(fmt_layer);

    if !config.enabled
    {
        // Telemetry disabled: only local stderr logging.
        registry.init();
        return Ok(TelemetryGuard {
            tracer_provider: None,
            meter_provider:  None,
            logger_provider: None,
        });
    }

    // ── trace provider ─────────────────────────────────────────
    let trace_exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .with_timeout(config.timeout)
        .build()
        .map_err(|e| TelemetryError::TraceExporter(e.to_string()))?;

    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(trace_exporter)
        .with_resource(resource.clone())
        .build();

    let tracer = tracer_provider.tracer("pk");
    let otel_layer = OpenTelemetryLayer::new(tracer);

    // ── metrics provider ───────────────────────────────────────
    let metrics_exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .with_timeout(config.timeout)
        .build()
        .map_err(|e| TelemetryError::MetricsExporter(e.to_string()))?;

    let meter_provider = SdkMeterProvider::builder()
        .with_periodic_exporter(metrics_exporter)
        .with_resource(resource.clone())
        .build();

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    // ── logs provider (optional) ───────────────────────────────
    let logger_provider = if config.logs_enabled
    {
        let logs_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(&config.endpoint)
            .with_timeout(config.timeout)
            .build()
            .map_err(|e| TelemetryError::LogsExporter(e.to_string()))?;

        let lp = SdkLoggerProvider::builder()
            .with_batch_exporter(logs_exporter)
            .with_resource(resource)
            .build();

        let log_layer = OpenTelemetryTracingBridge::new(&lp);
        registry.with(otel_layer).with(log_layer).init();
        Some(lp)
    }
    else
    {
        registry.with(otel_layer).init();
        None
    };

    Ok(TelemetryGuard {
        tracer_provider: Some(tracer_provider),
        meter_provider: Some(meter_provider),
        logger_provider,
    })
}
