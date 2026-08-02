//! `indicatif`-based implementation of [`ProgressReporter`].
//!
//! Creates a single [`MultiProgress`] per CLI invocation. Each task
//! becomes a `ProgressBar` whose style is chosen based on whether
//! the total is known (bar) or unknown (spinner).

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rspk_core::{ProgressReporter, ProgressTask};
use std::sync::Arc;

/// A progress reporter backed by `indicatif`.
///
/// Safe to clone cheaply (`Arc` internally) and share across tasks.
#[derive(Debug, Clone)]
pub struct IndicatifReporter
{
    multi: Arc<MultiProgress>,
}

impl IndicatifReporter
{
    /// Creates a new reporter, installing the `MultiProgress` on
    /// stderr.
    pub fn new() -> Self
    {
        Self {
            multi: Arc::new(MultiProgress::new()),
        }
    }
}

impl Default for IndicatifReporter
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl ProgressReporter for IndicatifReporter
{
    fn new_task(&self, name: String) -> Arc<dyn ProgressTask>
    {
        // Start as a spinner; callers switch to bar via `set_bar`
        // when the total becomes known.
        let pb = self.multi.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {prefix:.bold} {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓",
                ]),
        );
        pb.set_prefix(name);
        pb.enable_steady_tick(std::time::Duration::from_millis(100));
        Arc::new(IndicatifTask { pb })
    }

    fn println(&self, message: String)
    {
        let _ = self.multi.println(message);
    }
}

#[derive(Debug)]
struct IndicatifTask
{
    pb: ProgressBar,
}

impl ProgressTask for IndicatifTask
{
    fn set_spinner(&self, message: &str)
    {
        // If we were in bar mode, switch back to spinner.
        self.pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {prefix:.bold} {wide_msg}")
                .unwrap()
                .tick_strings(&[
                    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✓",
                ]),
        );
        self.pb.set_message(message.to_string());
    }

    fn set_bar(&self, total: u64, message: &str)
    {
        self.pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{prefix:.bold} [{bar:30.cyan/dim}] {pos:>3}/{len:<3} \
                     {msg}",
                )
                .unwrap()
                .progress_chars("██▉▊▋▌▍▎▏  "),
        );
        self.pb.set_length(total);
        self.pb.set_message(message.to_string());
    }

    fn set_position(&self, position: u64)
    {
        self.pb.set_position(position);
    }

    fn inc(&self, delta: u64)
    {
        self.pb.inc(delta);
    }

    fn set_message(&self, message: String)
    {
        self.pb.set_message(message);
    }

    fn finish_with_message(&self, message: String)
    {
        self.pb.finish_with_message(message);
    }

    fn finish_and_clear(&self)
    {
        self.pb.finish_and_clear();
    }
}
