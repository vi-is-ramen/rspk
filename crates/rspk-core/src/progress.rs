//! Progress reporting abstraction.
//!
//! Package managers report progress through [`ProgressReporter`]
//! instances obtained from [`ExecutionContext`](crate::ExecutionContext).
//! The CLI layer provides a concrete implementation using `indicatif`,
//! while RPC and test environments use a no-op implementation.
//!
//! This keeps manager code UI-agnostic: a manager just calls
//! `task.set_message("downloading foo")` and never knows whether
//! the user sees a TTY bar, a JSON event stream, or nothing at all.

use std::fmt::Debug;
use std::sync::Arc;

/// A long-running task whose progress is being reported.
///
/// Obtained from [`ProgressReporter::new_task`]. Each task corresponds
/// to a single visual element (a progress bar or spinner) in the UI.
pub trait ProgressTask: Send + Sync + Debug
{
    /// Switches the task to indeterminate (spinner) mode with the
    /// given message.
    fn set_spinner(&self, message: &str);

    /// Switches the task to determinate (progress bar) mode with a
    /// known total length.
    fn set_bar(&self, total: u64, message: &str);

    /// Sets the absolute position of a determinate task.
    fn set_position(&self, position: u64);

    /// Increments the position by `delta`.
    fn inc(&self, delta: u64);

    /// Updates the textual message without changing the position.
    fn set_message(&self, message: String);

    /// Marks the task as finished with a final message.
    fn finish_with_message(&self, message: String);

    /// Marks the task as finished and removes it from the UI.
    fn finish_and_clear(&self);
}

/// Factory for progress tasks.
///
/// The CLI creates one reporter per command invocation and shares it
/// (via `Arc`) across all managers that run during that invocation.
pub trait ProgressReporter: Send + Sync + Debug
{
    /// Creates a new task with the given name.
    ///
    /// The name is displayed as a prefix on the progress bar/spinner
    /// so the user can see which manager or package is being worked on.
    fn new_task(&self, name: String) -> Arc<dyn ProgressTask>;

    /// Prints a line of text without disrupting active progress bars.
    ///
    /// Use this instead of `println!` for any informational output
    /// that may appear while tasks are running.
    fn println(&self, message: String);
}

/// A no-op reporter that discards all events.
///
/// Useful for tests, RPC mode (where stdout is reserved for JSON),
/// and any environment where progress UI is unwanted.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullProgressReporter;

impl ProgressReporter for NullProgressReporter
{
    fn new_task(&self, _name: String) -> Arc<dyn ProgressTask>
    {
        Arc::new(NullProgressTask)
    }

    fn println(&self, _message: String) {}
}

#[derive(Debug, Clone, Copy)]
struct NullProgressTask;

impl ProgressTask for NullProgressTask
{
    fn set_spinner(&self, _message: &str) {}
    fn set_bar(&self, _total: u64, _message: &str) {}
    fn set_position(&self, _position: u64) {}
    fn inc(&self, _delta: u64) {}
    fn set_message(&self, _message: String) {}
    fn finish_with_message(&self, _message: String) {}
    fn finish_and_clear(&self) {}
}
