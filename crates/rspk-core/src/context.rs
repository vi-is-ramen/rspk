//! Execution context for package manager operations.

use crate::progress::{NullProgressReporter, ProgressReporter};
use std::sync::Arc;

/// Runtime configuration passed to package manager operations.
///
/// This context carries cross-cutting concerns like dry-run mode
/// and progress reporting that affect how managers execute their
/// commands without requiring each manager to maintain its own state.
#[derive(Debug, Clone)]
pub struct ExecutionContext
{
    /// When true, operations should display what they would do
    /// without actually modifying the system.
    pub dry_run: bool,

    /// When true, auto-select the first available manager without
    /// prompting the user for confirmation.
    pub quiet: bool,

    /// Progress reporter used by long-running operations.
    ///
    /// Always present; defaults to a no-op reporter when no UI is
    /// available (tests, RPC mode, etc.).
    pub progress: Arc<dyn ProgressReporter>,
}

impl Default for ExecutionContext
{
    fn default() -> Self
    {
        Self {
            dry_run:  false,
            quiet:    false,
            progress: Arc::new(NullProgressReporter),
        }
    }
}

impl ExecutionContext
{
    /// Creates a new execution context with default settings.
    pub fn new() -> Self
    {
        Self::default()
    }

    /// Creates an execution context with dry-run enabled.
    pub fn dry_run() -> Self
    {
        Self {
            dry_run:  true,
            quiet:    false,
            progress: Arc::new(NullProgressReporter),
        }
    }

    /// Creates an execution context for actual execution.
    pub fn real() -> Self
    {
        Self {
            dry_run:  false,
            quiet:    false,
            progress: Arc::new(NullProgressReporter),
        }
    }

    /// Replaces the progress reporter with the given one.
    #[must_use]
    pub fn with_progress(mut self, reporter: Arc<dyn ProgressReporter>)
    -> Self
    {
        self.progress = reporter;
        self
    }
}
