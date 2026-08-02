//! Application-level metrics.
//!
//! All metric names follow the OpenTelemetry semantic conventions and
//! use the `pk.` namespace to avoid collisions.

use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use opentelemetry::{KeyValue, global};
use std::time::Instant;

/// Central handle to all `pk` metrics.
///
/// Cheap to clone — the underlying instruments are held by the global
/// meter provider and shared across clones.
#[derive(Debug, Clone)]
pub struct Metrics
{
    /// Counter: `pk.operations.total` — every CLI invocation.
    pub operations_total:         Counter<u64>,
    /// Counter: `pk.manager.operations.total` — operations per manager.
    pub manager_operations_total: Counter<u64>,
    /// Counter: `pk.packages.installed` — packages actually installed.
    pub packages_installed:       Counter<u64>,
    /// Counter: `pk.packages.removed` — packages removed.
    pub packages_removed:         Counter<u64>,
    /// Counter: `pk.packages.upgraded` — packages upgraded.
    pub packages_upgraded:        Counter<u64>,
    /// Counter: `pk.errors.total` — errors by category.
    pub errors_total:             Counter<u64>,
    /// Histogram: `pk.operation.duration_seconds` — duration of CLI commands.
    pub operation_duration:       Histogram<f64>,
    /// Histogram: `pk.manager.call.duration_seconds` — per-manager call
    /// latency.
    pub manager_call_duration:    Histogram<f64>,
    /// Gauge: `pk.managers.available` — how many managers were discovered.
    pub managers_available:       UpDownCounter<i64>,
}

impl Default for Metrics
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl Metrics
{
    /// Creates a new metrics handle using the global meter provider.
    pub fn new() -> Self
    {
        let meter = global::meter("pk");
        Self::from_meter(&meter)
    }

    /// Creates a metrics handle from an explicit [`Meter`].
    pub fn from_meter(meter: &Meter) -> Self
    {
        Self {
            operations_total: meter
                .u64_counter("pk.operations.total")
                .with_description("Total number of pk CLI invocations")
                .with_unit("{operation}")
                .build(),

            manager_operations_total: meter
                .u64_counter("pk.manager.operations.total")
                .with_description("Operations executed per package manager")
                .with_unit("{operation}")
                .build(),

            packages_installed: meter
                .u64_counter("pk.packages.installed")
                .with_description("Packages installed successfully")
                .with_unit("{package}")
                .build(),

            packages_removed: meter
                .u64_counter("pk.packages.removed")
                .with_description("Packages removed successfully")
                .with_unit("{package}")
                .build(),

            packages_upgraded: meter
                .u64_counter("pk.packages.upgraded")
                .with_description("Packages upgraded successfully")
                .with_unit("{package}")
                .build(),

            errors_total: meter
                .u64_counter("pk.errors.total")
                .with_description("Errors encountered, by category")
                .with_unit("{error}")
                .build(),

            operation_duration: meter
                .f64_histogram("pk.operation.duration_seconds")
                .with_description("Duration of pk CLI commands")
                .with_unit("s")
                .build(),

            manager_call_duration: meter
                .f64_histogram("pk.manager.call.duration_seconds")
                .with_description("Duration of individual manager calls")
                .with_unit("s")
                .build(),

            managers_available: meter
                .i64_up_down_counter("pk.managers.available")
                .with_description("Number of discovered package managers")
                .with_unit("{manager}")
                .build(),
        }
    }

    /// Records the start of a CLI operation.
    pub fn record_operation(&self, command: &str, dry_run: bool)
    {
        self.operations_total.add(
            1,
            &[
                KeyValue::new("command", command.to_string()),
                KeyValue::new("dry_run", dry_run),
            ],
        );
    }

    /// Records a manager-level operation.
    pub fn record_manager_operation(
        &self,
        manager: &str,
        capability: &str,
        duration_secs: f64,
        success: bool,
    )
    {
        let attrs = [
            KeyValue::new("manager", manager.to_string()),
            KeyValue::new("capability", capability.to_string()),
            KeyValue::new("success", success),
        ];
        self.manager_operations_total.add(1, &attrs);
        self.manager_call_duration.record(duration_secs, &attrs);
    }

    /// Records a package installation.
    pub fn record_install(&self, manager: &str)
    {
        self.packages_installed
            .add(1, &[KeyValue::new("manager", manager.to_string())]);
    }

    /// Records a package removal.
    pub fn record_remove(&self, manager: &str)
    {
        self.packages_removed
            .add(1, &[KeyValue::new("manager", manager.to_string())]);
    }

    /// Records a package upgrade.
    pub fn record_upgrade(&self, manager: &str)
    {
        self.packages_upgraded
            .add(1, &[KeyValue::new("manager", manager.to_string())]);
    }

    /// Records an error.
    pub fn record_error(&self, category: &str, manager: Option<&str>)
    {
        let mut attrs = vec![KeyValue::new("category", category.to_string())];
        if let Some(m) = manager
        {
            attrs.push(KeyValue::new("manager", m.to_string()));
        }
        self.errors_total.add(1, &attrs);
    }

    /// Records the total duration of a CLI command.
    pub fn record_duration(&self, command: &str, secs: f64)
    {
        self.operation_duration
            .record(secs, &[KeyValue::new("command", command.to_string())]);
    }

    /// Sets the number of discovered managers.
    pub fn set_managers_available(&self, count: i64)
    {
        // UpDownCounter has no `set`, so we record the delta from 0.
        self.managers_available.add(count, &[]);
    }
}

/// Helper to time a block and return the elapsed seconds.
///
/// Cheap, copyable (it only stores an `Instant`). Use it together with
/// [`Metrics::record_manager_operation`] or
/// [`Metrics::record_duration`].
///
/// # Example
///
/// ```ignore
/// let t = Timer::start();
/// do_work().await;
/// metrics.record_duration("install", t.elapsed_secs());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Timer
{
    start: Instant,
}

impl Timer
{
    /// Starts a new timer.
    pub fn start() -> Self
    {
        Self {
            start: Instant::now(),
        }
    }

    /// Returns elapsed seconds since [`start`](Self::start).
    pub fn elapsed_secs(&self) -> f64
    {
        self.start.elapsed().as_secs_f64()
    }
}

impl Default for Timer
{
    fn default() -> Self
    {
        Self::start()
    }
}
