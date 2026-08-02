//! Command execution engine for package managers.
//!
//! This crate provides [`CommandBuilder`] for safely spawning and
//! managing external processes, and [`progress_parser`] for
//! extracting progress information from command output.
//!
//! Registry API clients (AUR, crates.io, Repology, RubyGems) have
//! been moved to the `rspk-regs` crate.
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod progress_parser;

use rspk_core::ExecutionContext;
use std::collections::HashMap;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub use progress_parser::ProgressEvent;

/// Errors that can occur during command execution.
#[derive(Debug, Error)]
pub enum ExecutionError
{
    /// The command timed out.
    #[error("command timed out after {0:?}")]
    Timeout(Duration),
    /// The command exited with a non-zero status.
    #[error("command failed with exit code {exit_code}: {stderr}")]
    NonZeroExit
    {
        /// The exit code of the process.
        exit_code: i32,
        /// The stderr output of the process.
        stderr:    String,
    },
    /// The command could not be started.
    #[error("failed to start command: {0}")]
    StartError(#[from] std::io::Error),
    /// Output was not valid UTF-8.
    #[error("invalid UTF-8 in command output")]
    InvalidUtf8,
    /// Unclassifiable error
    #[error("Error: {0}")]
    OtherError(String),
}

impl From<ExecutionError> for rspk_core::Error
{
    fn from(value: ExecutionError) -> Self
    {
        rspk_core::Error::ExecutionError(value.to_string())
    }
}

/// The output of a command execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput
{
    /// Standard output of the command.
    pub stdout:    String,
    /// Standard error of the command.
    pub stderr:    String,
    /// The exit code of the command.
    pub exit_code: i32,
}

/// A builder for executing external commands safely.
#[derive(Debug, Clone)]
pub struct CommandBuilder
{
    program:    String,
    args:       Vec<String>,
    env:        HashMap<String, String>,
    timeout:    Duration,
    use_sudo:   bool,
    dry_run:    bool,
    /// Optional label shown on the progress bar (e.g. manager name).
    task_label: Option<String>,
}

impl CommandBuilder
{
    /// Creates a new command builder for the given program.
    pub fn new<S: Into<String>>(program: S) -> Self
    {
        Self {
            program:    program.into(),
            args:       Vec::new(),
            env:        HashMap::new(),
            timeout:    Duration::from_secs(300),
            use_sudo:   false,
            dry_run:    false,
            task_label: None,
        }
    }

    /// Adds an argument to the command.
    #[must_use]
    pub fn arg<S: Into<String>>(mut self, arg: S) -> Self
    {
        self.args.push(arg.into());
        self
    }

    /// Adds multiple arguments to the command.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets an environment variable for the command.
    #[must_use]
    pub fn env<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Sets the timeout for command execution.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self
    {
        self.timeout = timeout;
        self
    }

    /// Enables sudo elevation for the command.
    #[must_use]
    pub fn with_sudo(mut self) -> Self
    {
        self.use_sudo = true;
        self
    }

    /// Enables dry-run mode. The command will be displayed but not executed.
    #[must_use]
    pub fn dry_run(mut self, dry_run: bool) -> Self
    {
        self.dry_run = dry_run;
        self
    }

    /// Sets a label for the progress bar shown while this command runs.
    ///
    /// If set, `run_streaming` will create a progress task with this
    /// label and update it as output arrives. If unset, a default
    /// label based on the program name is used.
    #[must_use]
    pub fn with_progress_label<S: Into<String>>(mut self, label: S) -> Self
    {
        self.task_label = Some(label.into());
        self
    }

    /// Formats the command as a shell-safe string for display.
    fn format_command(&self) -> String
    {
        let mut parts = Vec::new();
        for (key, value) in &self.env
        {
            parts.push(format!(
                "{}={}",
                key,
                shell_escape::escape(value.into())
            ));
        }
        if self.use_sudo
        {
            parts.push("sudo".to_string());
        }
        parts.push(
            shell_escape::escape(self.program.clone().into()).into_owned(),
        );
        for arg in &self.args
        {
            parts.push(shell_escape::escape(arg.clone().into()).into_owned());
        }
        parts.join(" ")
    }

    /// Executes the command, capturing all output.
    ///
    /// Suitable for quick commands (`--version`, `list`, `search`)
    /// where progress reporting is unnecessary.
    pub async fn run(self) -> Result<CommandOutput, ExecutionError>
    {
        if self.dry_run
        {
            eprintln!("[dry-run] {}", self.format_command());
            return Ok(CommandOutput {
                stdout:    String::new(),
                stderr:    String::new(),
                exit_code: 0,
            });
        }
        let mut cmd = self.build_command();
        let output = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .map_err(|_| ExecutionError::Timeout(self.timeout))?
            .map_err(ExecutionError::StartError)?;
        Ok(CommandOutput {
            stdout:    String::from_utf8_lossy(&output.stdout).to_string(),
            stderr:    String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Executes the command with live streaming of stdout to a
    /// progress task obtained from the given execution context.
    ///
    /// Each line of stdout is passed through [`progress_parser`]
    /// which detects progress patterns (percentages, download
    /// stages, spinner-worthy activity) and updates the task
    /// accordingly.
    ///
    /// Use this for long-running commands like `install`, `upgrade`,
    /// `remove` where the user benefits from visual feedback.
    #[allow(clippy::too_many_lines, clippy::missing_errors_doc)]
    pub async fn run_streaming(
        self,
        ctx: &ExecutionContext,
    ) -> Result<CommandOutput, ExecutionError>
    {
        if self.dry_run
        {
            ctx.progress
                .println(format!("[dry-run] {}", self.format_command()));
            return Ok(CommandOutput {
                stdout:    String::new(),
                stderr:    String::new(),
                exit_code: 0,
            });
        }

        let label = self
            .task_label
            .clone()
            .unwrap_or_else(|| self.program.clone());
        let task = ctx.progress.new_task(label);
        task.set_spinner("starting");

        let mut cmd = self.build_command();
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(ExecutionError::StartError)?;

        let stdout = child
            .stdout
            .take()
            .ok_or(ExecutionError::OtherError("stdout piped".to_string()))?;
        let stderr_handle = child
            .stderr
            .take()
            .ok_or(ExecutionError::OtherError("stderr piped".to_string()))?;

        // Collect stderr in background.
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr_handle);
            let mut buf = String::new();
            let mut collected = String::new();
            loop
            {
                buf.clear();
                let n = reader.read_line(&mut buf).await.unwrap_or(0);
                if n == 0
                {
                    break;
                }
                collected.push_str(&buf);
            }
            collected
        });

        // Stream stdout, parse progress, accumulate for return.
        let mut reader = BufReader::new(stdout);
        let mut stdout_collected = String::new();
        let mut line_buf = String::new();
        let _start = std::time::Instant::now();

        loop
        {
            line_buf.clear();
            let n = tokio::time::timeout(
                self.timeout,
                reader.read_line(&mut line_buf),
            )
            .await
            .map_err(|_| ExecutionError::Timeout(self.timeout))?
            .map_err(ExecutionError::StartError)?;
            if n == 0
            {
                break;
            }
            stdout_collected.push_str(&line_buf);

            match progress_parser::parse_line(&line_buf)
            {
                progress_parser::ProgressEvent::Percent(pct, msg) =>
                {
                    task.set_bar(100, &msg);
                    task.set_position(u64::from(pct));
                },
                progress_parser::ProgressEvent::Stage(msg) =>
                {
                    task.set_spinner(&msg);
                },
                progress_parser::ProgressEvent::BytesDownloaded(
                    bytes,
                    total,
                    msg,
                ) =>
                {
                    if let Some(t) = total
                    {
                        task.set_bar(t, &msg);
                        task.set_position(bytes);
                    }
                    else
                    {
                        task.set_spinner(&format!(
                            "{} ({})",
                            msg,
                            format_bytes(bytes)
                        ));
                    }
                },
                progress_parser::ProgressEvent::None =>
                {
                    let trimmed = line_buf.trim();
                    if !trimmed.is_empty()
                    {
                        let short = if trimmed.len() > 60
                        {
                            format!("{}…", &trimmed[..57])
                        }
                        else
                        {
                            trimmed.to_string()
                        };
                        task.set_spinner(&short);
                    }
                },
            }
        }

        let status = tokio::time::timeout(self.timeout, child.wait())
            .await
            .map_err(|_| ExecutionError::Timeout(self.timeout))?
            .map_err(ExecutionError::StartError)?;
        let stderr_collected = stderr_task.await.unwrap_or_default();

        if status.success()
        {
            task.finish_with_message("done".to_string());
        }
        else
        {
            task.finish_with_message("failed".to_string());
        }

        let exit_code = status.code().unwrap_or(-1);
        if exit_code != 0
        {
            return Err(ExecutionError::NonZeroExit {
                exit_code,
                stderr: stderr_collected,
            });
        }

        Ok(CommandOutput {
            stdout: stdout_collected,
            stderr: stderr_collected,
            exit_code,
        })
    }

    fn build_command(&self) -> Command
    {
        let mut cmd = if self.use_sudo
        {
            let mut c = Command::new("sudo");
            c.arg(&self.program);
            c.args(&self.args);
            c
        }
        else
        {
            let mut c = Command::new(&self.program);
            c.args(&self.args);
            c
        };
        for (key, value) in &self.env
        {
            cmd.env(key, value);
        }
        cmd
    }
}

#[allow(clippy::cast_precision_loss)]
fn format_bytes(n: u64) -> String
{
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if n >= GB
    {
        format!("{:.1} GB", n as f64 / GB as f64)
    }
    else if n >= MB
    {
        format!("{:.1} MB", n as f64 / MB as f64)
    }
    else if n >= KB
    {
        format!("{:.1} KB", n as f64 / KB as f64)
    }
    else
    {
        format!("{n} B")
    }
}
