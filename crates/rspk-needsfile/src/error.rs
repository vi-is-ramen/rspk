//! Error types for Needsfile parsing.

use annotate_snippets::{Level, Renderer, Snippet};

/// A parse error within a Needsfile.
#[derive(Debug, Clone)]
pub struct ParseError
{
    /// Byte offset from the start of the file.
    pub offset:  usize,
    /// Length (in bytes) of the erroneous span. At least 1.
    pub length:  usize,
    /// Human-readable description.
    pub message: String,
}

/// Error returned when a Needsfile cannot be read or parsed.
#[derive(Debug)]
pub struct NeedsfileError
{
    /// Full source text of the file.
    pub source:   String,
    /// Display path of the file.
    pub path:     String,
    /// All parse errors found, in source order.
    pub errors:   Vec<ParseError>,
    /// I/O error, if the file could not be read.
    pub io_error: Option<std::io::Error>,
}

impl NeedsfileError
{
    /// Renders the error using `annotate-snippets`.
    pub fn render(&self) -> String
    {
        if let Some(ref io) = self.io_error
        {
            return format!("Failed to read '{}': {}", self.path, io);
        }
        let renderer = Renderer::styled();
        let mut output = String::new();
        for error in &self.errors
        {
            let end = (error.offset + error.length).min(self.source.len());
            let span = error.offset..end;
            let message =
                Level::Error.title("invalid Needsfile syntax").snippet(
                    Snippet::source(&self.source)
                        .origin(&self.path)
                        .fold(true)
                        .annotation(
                            Level::Error.span(span).label(&error.message),
                        ),
                );
            output.push_str(&renderer.render(message).to_string());
            output.push('\n');
        }
        let total = self.errors.len();
        output.push_str(&format!(
            "Found {} parse error{} in '{}'.\n",
            total,
            if total == 1 { "" } else { "s" },
            self.path
        ));
        output
    }
}

impl std::fmt::Display for NeedsfileError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result
    {
        write!(f, "{}", self.render())
    }
}

impl std::error::Error for NeedsfileError {}
